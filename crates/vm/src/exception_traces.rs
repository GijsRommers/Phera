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
            entry.insert(ArrayKey::str("args"), arguments);
            let warning = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == callee_index);
            let mut callbacks = self
                .native_callback_sites
                .iter()
                .rev()
                .filter(|site| site.boundary == callee_index)
                .peekable();
            // At the end of the request PHP calls destructors from no code
            // at all: "[internal function]".
            let from_shutdown = self.request_end.shutting_down && callee_index == 1;
            if warning.is_some() || callbacks.peek().is_some() || from_shutdown {
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
            } else {
                for site in callbacks {
                    append_native(&mut trace, &site.native, &caller.unit().file, caller.line());
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
    /// PHP snapshots live parameter values, flattening variadics and wrapping secrets.
    fn trace_arguments(&mut self, depth: usize) -> Value {
        let Some(frame) = self.frame_at(depth) else {
            return Value::from(PhpArray::new());
        };
        let mut snapshot = Vec::new();
        let params = &frame.function().parameters;
        for (index, parameter) in params.iter().enumerate().take(frame.supplied_arguments()) {
            let Some(Some(value)) = frame.slots(&self.registers).get(index) else {
                continue;
            };
            let sensitive = crate::sensitive_parameters::is_sensitive(frame, index);
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
            for value in self
                .registers
                .window(frame.base() + frame.count(), frame.extras.arguments.extra)
            {
                let value = value.as_ref().unwrap_or(&Value::Null);
                snapshot.push((None, value.deref().clone(), false));
            }
        }
        let mut arguments = PhpArray::new();
        for (key, value, sensitive) in snapshot {
            let value = if sensitive {
                self.sensitive_trace_value(value)
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
