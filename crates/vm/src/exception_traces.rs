//! Capture PHP array-shaped traces while the heap frames still own their arguments.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, frame::Frame};

impl Vm {
    /// Frame entries point at the caller's call instruction and omit the main frame.
    pub(crate) fn throwable_trace(&mut self, origin: crate::exceptions::TraceOrigin) -> Value {
        let mut trace = PhpArray::new();
        if let Some(native) = matches!(origin, crate::exceptions::TraceOrigin::Engine)
            .then(|| self.failed_native.take())
            .flatten()
        {
            let location = self.location();
            let mut entry = PhpArray::new();
            entry.insert(ArrayKey::str("file"), Value::from(location.file));
            entry.insert(ArrayKey::str("line"), Value::Int(location.line as i64));
            if let Some((class, method)) = native.name.split_once("::") {
                entry.insert(ArrayKey::str("function"), Value::from(method));
                entry.insert(ArrayKey::str("class"), Value::from(class));
                entry.insert(ArrayKey::str("type"), Value::from("->"));
            } else {
                entry.insert(ArrayKey::str("function"), Value::from(native.name));
            }
            entry.insert(ArrayKey::str("args"), native.arguments);
            let _ = trace.push(Value::Array(Rc::new(entry)));
        }
        for ((_, caller), (callee_index, callee)) in self
            .call_stack
            .iter()
            .enumerate()
            .rev()
            .skip(1)
            .zip(self.call_stack.iter().enumerate().rev())
        {
            let mut entry = PhpArray::new();
            entry.insert(ArrayKey::str("file"), Value::from(caller.unit.file.clone()));
            entry.insert(ArrayKey::str("line"), Value::Int(caller.line() as i64));
            if callee.function_id.is_none()
                && let Some(phera_bytecode::Instruction::Include { kind, .. }) = caller
                    .function()
                    .instructions
                    .get(caller.ip.saturating_sub(1))
            {
                let name = match kind {
                    phera_bytecode::IncludeKind::Include => "include",
                    phera_bytecode::IncludeKind::IncludeOnce => "include_once",
                    phera_bytecode::IncludeKind::Require => "require",
                    phera_bytecode::IncludeKind::RequireOnce => "require_once",
                };
                let arguments = PhpArray::new();
                entry.insert(ArrayKey::str("function"), Value::from(name));
                entry.insert(ArrayKey::str("args"), Value::Array(Rc::new(arguments)));
                let _ = trace.push(Value::Array(Rc::new(entry)));
                continue;
            }
            let function = &callee.function().name;
            let name = function.rsplit("::").next().unwrap_or(function);
            entry.insert(ArrayKey::str("function"), Value::from(name));
            if let Some((class, _)) = function.split_once("::") {
                entry.insert(ArrayKey::str("class"), Value::from(class));
                let kind = if callee
                    .method
                    .as_ref()
                    .is_some_and(|context| context.receiver.is_some())
                {
                    "->"
                } else {
                    "::"
                };
                entry.insert(ArrayKey::str("type"), Value::from(kind));
            }
            entry.insert(ArrayKey::str("args"), trace_arguments(self, callee));
            let native = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == callee_index);
            if native.is_some() {
                entry.remove(&ArrayKey::str("file"));
                entry.remove(&ArrayKey::str("line"));
            }
            let _ = trace.push(Value::Array(Rc::new(entry)));
            if let Some(site) = native {
                let mut entry = PhpArray::new();
                entry.insert(
                    ArrayKey::str("file"),
                    Value::from(site.location.file.clone()),
                );
                entry.insert(ArrayKey::str("line"), Value::Int(site.location.line as i64));
                entry.insert(
                    ArrayKey::str("function"),
                    Value::from(site.native.name.clone()),
                );
                entry.insert(ArrayKey::str("args"), site.native.arguments.clone());
                let _ = trace.push(Value::Array(Rc::new(entry)));
            }
        }
        if let Some(frame) = self.call_stack.iter().next()
            && frame.function_id.is_some()
        {
            let mut entry = PhpArray::new();
            entry.insert(
                ArrayKey::str("function"),
                Value::from(frame.function().name.clone()),
            );
            entry.insert(ArrayKey::str("args"), trace_arguments(self, frame));
            let _ = trace.push(Value::Array(Rc::new(entry)));
        }
        Value::Array(Rc::new(trace))
    }
}

/// PHP snapshots live, coerced parameters and flattens a variadic argument array.
fn trace_arguments(vm: &Vm, frame: &Frame) -> Value {
    let mut arguments = PhpArray::new();
    for (index, parameter) in frame
        .function()
        .parameters
        .iter()
        .enumerate()
        .take(frame.supplied_arguments)
    {
        let Some(Some(value)) = frame.slots(&vm.registers).get(index) else {
            continue;
        };
        if parameter.variadic {
            if let Value::Array(values) = &*value.deref() {
                for (key, value) in values.iter() {
                    match key {
                        ArrayKey::Int(_) => {
                            let _ = arguments.push(value.deref().clone());
                        }
                        key => {
                            arguments.insert(key.clone(), value.deref().clone());
                        }
                    }
                }
            }
        } else {
            let _ = arguments.push(value.deref().clone());
        }
    }
    if !frame
        .function()
        .parameters
        .iter()
        .any(|parameter| parameter.variadic)
    {
        for value in frame
            .arguments
            .iter()
            .skip(frame.function().parameters.len())
        {
            let _ = arguments.push(value.deref().clone());
        }
    }
    Value::Array(Rc::new(arguments))
}

/// A native warning invokes a PHP handler through an internal frame at this stack boundary.
pub(crate) struct WarningSite {
    pub(crate) boundary: usize,
    pub(crate) native: crate::error_traces::NativeTrace,
    pub(crate) location: crate::diagnostics::SourceLocation,
}
