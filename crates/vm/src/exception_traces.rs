//! Capture PHP array-shaped traces while the heap frames still own their arguments.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, calls::frame::FrameView};

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
            .frames()
            .enumerate()
            .rev()
            .skip(1)
            .zip(self.frames().enumerate().rev())
        {
            let mut entry = PhpArray::new();
            entry.insert(
                ArrayKey::str("file"),
                Value::from(caller.unit().file.clone()),
            );
            entry.insert(ArrayKey::str("line"), Value::Int(caller.line() as i64));
            if callee.function_id().is_none()
                && let Some(name) = scoped_unit_name(caller)
            {
                let arguments = PhpArray::new();
                entry.insert(ArrayKey::str("function"), Value::from(name));
                entry.insert(ArrayKey::str("args"), Value::Array(Rc::new(arguments)));
                let _ = trace.push(Value::Array(Rc::new(entry)));
                continue;
            }
            let function = &callee.function().name;
            let closure = function.starts_with("{closure:");
            let name = if closure {
                function.as_str()
            } else {
                function.rsplit("::").next().unwrap_or(function)
            };
            entry.insert(ArrayKey::str("function"), Value::from(name));
            let class = if closure {
                callee.scope().map(|scope| scope.name.as_str())
            } else {
                function.split_once("::").map(|(class, _)| class)
            };
            if let Some(class) = class {
                entry.insert(ArrayKey::str("class"), Value::from(class));
                let kind = if callee.this().is_some() { "->" } else { "::" };
                entry.insert(ArrayKey::str("type"), Value::from(kind));
            }
            entry.insert(ArrayKey::str("args"), trace_arguments(self, callee));
            let warning = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == callee_index);
            let native_callback = self
                .native_callback_sites
                .iter()
                .rev()
                .find(|site| site.boundary == callee_index);
            if warning.is_some() || native_callback.is_some() {
                entry.remove(&ArrayKey::str("file"));
                entry.remove(&ArrayKey::str("line"));
            }
            let _ = trace.push(Value::Array(Rc::new(entry)));
            if let Some(site) = warning {
                append_native(
                    &mut trace,
                    &site.native,
                    &site.location.file,
                    site.location.line,
                );
            } else if let Some(site) = native_callback {
                append_native(&mut trace, &site.native, &caller.unit().file, caller.line());
            }
        }
        if let Some(frame) = self.frames().next()
            && frame.function_id().is_some()
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
fn trace_arguments(vm: &Vm, frame: FrameView<'_>) -> Value {
    let mut arguments = PhpArray::new();
    for (index, parameter) in frame
        .function()
        .parameters
        .iter()
        .enumerate()
        .take(frame.supplied_arguments())
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
                            arguments.insert(key, value.deref().clone());
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
        for value in vm
            .registers
            .window(frame.base() + frame.count(), frame.extras.arguments.extra)
        {
            let value = value.as_ref().unwrap_or(&Value::Null);
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

/// A nested unit is shown as the construct that entered it.
fn scoped_unit_name(frame: FrameView<'_>) -> Option<&'static str> {
    use phera_bytecode::{IncludeKind, Instruction};
    Some(
        match frame
            .function()
            .instructions
            .get(frame.ip().saturating_sub(1))?
        {
            Instruction::Eval { .. } => "eval",
            Instruction::Include { kind, .. } => match kind {
                IncludeKind::Include => "include",
                IncludeKind::IncludeOnce => "include_once",
                IncludeKind::Require => "require",
                IncludeKind::RequireOnce => "require_once",
            },
            _ => return None,
        },
    )
}

fn append_native(
    trace: &mut PhpArray,
    native: &crate::error_traces::NativeTrace,
    file: &str,
    line: usize,
) {
    let mut entry = PhpArray::new();
    entry.insert(ArrayKey::str("file"), Value::from(file));
    entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
    entry.insert(ArrayKey::str("function"), Value::from(native.name.clone()));
    entry.insert(ArrayKey::str("args"), native.arguments.clone());
    let _ = trace.push(Value::Array(Rc::new(entry)));
}
