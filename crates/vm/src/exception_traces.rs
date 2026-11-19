//! Capture PHP array-shaped traces while the heap frames still own their arguments.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, calls::frame::FrameView};

impl Vm {
    pub(crate) fn exception_location(&self) -> (Value, usize) {
        if let Some(location) = &self.diagnostic_location {
            (Value::from(location.file.as_str()), location.line)
        } else if let Some(frame) = self.current() {
            (frame.cache().trace_file(frame.unit()), frame.line())
        } else {
            (Value::from("Unknown"), 0)
        }
    }

    /// Frame entries point at the caller's call instruction and omit the main frame.
    pub(crate) fn throwable_trace(&mut self, origin: crate::exceptions::TraceOrigin) -> Value {
        if self.generator_traces.is_empty() {
            self.trace_frames::<false>(origin)
        } else {
            self.trace_frames::<true>(origin)
        }
    }

    fn trace_frames<const INCLUDE_GENERATORS: bool>(
        &mut self,
        origin: crate::exceptions::TraceOrigin,
    ) -> Value {
        let mut trace = PhpArray::new();
        if let Some(native) = matches!(origin, crate::exceptions::TraceOrigin::Engine)
            .then(|| self.failed_native.take())
            .flatten()
        {
            let (file, line) = self.exception_location();
            let mut entry = PhpArray::new();
            entry.insert(ArrayKey::str("file"), file);
            entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
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
        for callee_index in (1..self.call_stack.depth()).rev() {
            let arguments = self.trace_arguments(callee_index);
            let (Some(caller), Some(callee)) =
                (self.frame_at(callee_index - 1), self.frame_at(callee_index))
            else {
                continue;
            };
            let mut entry = PhpArray::new();
            entry.insert(
                ArrayKey::str("file"),
                caller.cache().trace_file(caller.unit()),
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
            let function = callee.function().name.as_str();
            let (class, name) = if function.starts_with("{closure:") {
                (callee.scope().map(|scope| scope.name.as_str()), function)
            } else if let Some((class, method)) = function.split_once("::") {
                if method.starts_with('$')
                    && let Some((class, method)) = self
                        .code
                        .function_name(callee.frame.function)
                        .split_once("::")
                {
                    (Some(class), method)
                } else {
                    (Some(class), method)
                }
            } else {
                (None, function)
            };
            entry.insert(ArrayKey::str("function"), Value::from(name));
            if let Some(class) = class {
                entry.insert(ArrayKey::str("class"), Value::from(class));
                let kind = if callee.this().is_some() { "->" } else { "::" };
                entry.insert(ArrayKey::str("type"), Value::from(kind));
            }
            entry.insert(ArrayKey::str("args"), arguments);
            let caller_function = caller.frame.function;
            let caller_line = caller.line();
            let caller_file = self.code.unit(caller_function).file.as_str();
            let warning = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == callee_index);
            let suspended_native = self
                .call_stack
                .extras(callee_index)
                .property_hook
                .as_deref()
                .and_then(crate::property_hook_continuation::HookContinuation::native_trace);
            let mut callbacks = self
                .native_callback_sites
                .iter()
                .rev()
                .filter(|site| site.boundary == callee_index)
                .peekable();
            let via_callback =
                warning.is_some() || suspended_native.is_some() || callbacks.peek().is_some();
            if INCLUDE_GENERATORS
                && let Some(site) = self
                    .generator_traces
                    .iter()
                    .rfind(|site| site.boundary == callee_index)
            {
                site.append_generator_trace(
                    &self.code,
                    &mut self.classes,
                    &mut trace,
                    entry,
                    (caller_file, caller_line),
                    via_callback,
                );
            } else {
                if via_callback {
                    entry.remove(&ArrayKey::str("file"));
                    entry.remove(&ArrayKey::str("line"));
                }
                let _ = trace.push(Value::Array(Rc::new(entry)));
            }
            if let Some(site) = warning {
                append_native(
                    &mut trace,
                    &site.native,
                    &site.location.file,
                    site.location.line,
                );
            } else {
                if let Some(native) = suspended_native {
                    append_native(&mut trace, native, caller_file, caller_line);
                }
                for site in callbacks {
                    append_native(&mut trace, &site.native, caller_file, caller_line);
                }
            }
        }
        if let Some(name) = self
            .frame_at(0)
            .filter(|frame| frame.function_id().is_some())
            .map(|frame| frame.function().name.clone())
        {
            let mut entry = PhpArray::new();
            entry.insert(ArrayKey::str("function"), Value::from(name));
            entry.insert(ArrayKey::str("args"), self.trace_arguments(0));
            let _ = trace.push(Value::Array(Rc::new(entry)));
        }
        Value::Array(Rc::new(trace))
    }
}

impl Vm {
    fn trace_arguments(&mut self, depth: usize) -> Value {
        let Some(frame) = self.frame_at(depth) else {
            return Value::from(PhpArray::new());
        };
        let snapshot = self.frame_trace_arguments(frame);
        materialize_trace_arguments(&mut self.classes, snapshot)
    }

    pub(crate) fn frame_trace_arguments(&self, frame: FrameView<'_>) -> TraceArguments {
        snapshot_arguments(
            &self.code,
            frame.frame.function,
            frame.supplied_arguments(),
            frame.slots(&self.registers),
            self.registers
                .window(frame.base() + frame.count(), frame.extras.arguments.extra),
        )
    }
}

pub(crate) fn materialize_trace_arguments(
    classes: &mut crate::classes::ClassTable,
    snapshot: TraceArguments,
) -> Value {
    let mut arguments = PhpArray::new();
    for (key, value, sensitive) in snapshot {
        let value = if sensitive {
            crate::sensitive_parameters::wrap_value(classes, value)
        } else {
            value
        };
        if let Some(key) = key {
            arguments.insert(key, value);
        } else {
            let _ = arguments.push(value);
        }
    }
    Value::from(arguments)
}

pub(crate) type TraceArguments = Vec<(Option<ArrayKey>, Value, bool)>;

/// Live and suspended frames expose the same coerced parameters, including named variadics.
pub(crate) fn snapshot_arguments(
    code: &crate::code_table::CodeTable,
    function: crate::code_table::FunctionIndex,
    supplied: usize,
    slots: &[Option<Value>],
    extra: &[Option<Value>],
) -> TraceArguments {
    let mut snapshot = Vec::new();
    let params = &code.function(function).parameters;
    for (index, (parameter, value)) in params.iter().zip(slots).take(supplied).enumerate() {
        let Some(value) = value else {
            continue;
        };
        let sensitive = crate::sensitive_parameters::is_sensitive_at(code, function, index);
        if parameter.variadic {
            if let Value::Array(items) = &*value.deref() {
                for (key, value) in items.iter() {
                    let name = match key {
                        ArrayKey::String(_) => Some(key),
                        _ => None,
                    };
                    snapshot.push((name, value.deref().clone(), sensitive));
                }
            }
        } else {
            snapshot.push((None, value.deref().clone(), sensitive));
        }
    }
    if !params.iter().any(|parameter| parameter.variadic) {
        for value in extra {
            let value = value.as_ref().unwrap_or(&Value::Null);
            snapshot.push((None, value.deref().clone(), false));
        }
    }
    snapshot
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
    if let Some((class, method)) = native.name.split_once("->") {
        entry.insert(ArrayKey::str("function"), Value::from(method));
        entry.insert(ArrayKey::str("class"), Value::from(class));
        entry.insert(ArrayKey::str("type"), Value::from("->"));
    } else {
        entry.insert(ArrayKey::str("function"), Value::from(native.name.clone()));
    }
    entry.insert(ArrayKey::str("args"), native.arguments.clone());
    let _ = trace.push(Value::Array(Rc::new(entry)));
}
