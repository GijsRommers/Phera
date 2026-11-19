//! `debug_backtrace()` and `$exception->getTrace()` read the same heap frames.

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
        Value::from(self.capture_trace(trace, false, true, 0))
    }

    /// Stop at the requested number of PHP-visible frames, including native callbacks.
    pub(crate) fn capture_trace(
        &mut self,
        mut trace: PhpArray,
        objects: bool,
        arguments: bool,
        limit: i32,
    ) -> PhpArray {
        let limit = match limit {
            0 => usize::MAX,
            positive if positive > 0 => positive as usize,
            _ => return trace,
        };
        let mut callbacks = self.native_callback_sites.len();
        let mut child_file = None;
        for depth in (0..self.call_stack.depth()).rev() {
            if trace.len() >= limit {
                break;
            }
            let warning = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == depth);
            let callback = callbacks
                .checked_sub(1)
                .and_then(|index| self.native_callback_sites.get(index))
                .filter(|site| site.boundary == depth);
            let internal = warning.is_some() || callback.is_some();
            let Some(callee) = self.frame_at(depth) else {
                break;
            };
            let caller = depth.checked_sub(1).and_then(|index| self.frame_at(index));
            let mut entry = PhpArray::new();
            if let Some(caller) = caller.filter(|_| !internal) {
                add_location(&mut entry, caller);
            }
            if callee.function_id().is_some() {
                add_function(&mut entry, callee, objects);
                if arguments {
                    entry.insert(ArrayKey::str("args"), self.trace_arguments(depth));
                }
            } else if let Some(kind) = caller.and_then(scoped_unit_name) {
                // The include path comes from its child frame's call site. PHP omits it
                // when the backtrace runs directly in the included top-level code.
                if kind != "eval"
                    && let Some(file) = child_file.take()
                {
                    let mut paths = PhpArray::new();
                    let _ = paths.push(file);
                    entry.insert(ArrayKey::str("args"), Value::from(paths));
                }
                entry.insert(ArrayKey::str("function"), Value::from(kind));
            } else {
                break;
            }
            child_file = entry.get(&ArrayKey::str("file")).cloned();
            let _ = trace.push(Value::from(entry));
            if trace.len() >= limit {
                break;
            }
            if let Some(site) = self
                .error_handlers
                .warning_site
                .as_ref()
                .filter(|site| site.boundary == depth)
            {
                append_native(
                    &mut trace,
                    &site.native,
                    Some((&site.location.file, site.location.line)),
                    arguments,
                );
            }
            while trace.len() < limit {
                let Some(index) = callbacks.checked_sub(1) else {
                    break;
                };
                let Some(site) = self
                    .native_callback_sites
                    .get(index)
                    .filter(|site| site.boundary == depth)
                else {
                    break;
                };
                callbacks = index;
                let nested = index
                    .checked_sub(1)
                    .and_then(|parent| self.native_callback_sites.get(parent))
                    .is_some_and(|parent| parent.boundary == depth);
                let location = depth
                    .checked_sub(1)
                    .and_then(|parent| self.frame_at(parent))
                    .filter(|_| !nested)
                    .map(|caller| (caller.unit().file.as_str(), caller.line()));
                append_native(&mut trace, &site.native, location, arguments);
            }
        }
        trace
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
            let value = frame
                .slots(&self.registers)
                .get(index)
                .and_then(Option::as_ref)
                .unwrap_or(&Value::Null);
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

fn add_location(entry: &mut PhpArray, caller: FrameView<'_>) {
    entry.insert(
        ArrayKey::str("file"),
        caller.cache().trace_file(caller.unit()),
    );
    entry.insert(ArrayKey::str("line"), Value::Int(caller.line() as i64));
}

fn add_function(entry: &mut PhpArray, callee: FrameView<'_>, objects: bool) {
    let function = &callee.function().name;
    let closure = function.starts_with("{closure:");
    let name = if closure {
        function.as_str()
    } else {
        function.rsplit("::").next().unwrap_or(function)
    };
    entry.insert(ArrayKey::str("function"), Value::from(name));
    let object = callee.this();
    let class = callee
        .scope()
        .map(|scope| scope.name.as_str())
        .or_else(|| {
            function
                .split_once("::")
                .filter(|_| !closure)
                .map(|(class, _)| class)
        })
        .or_else(|| object.map(|object| object.class.name()));
    if let Some(class) = class {
        entry.insert(ArrayKey::str("class"), Value::from(class));
        if objects && let Some(object) = object {
            entry.insert(ArrayKey::str("object"), Value::Object(Rc::clone(object)));
        }
        let kind = if object.is_some() { "->" } else { "::" };
        entry.insert(ArrayKey::str("type"), Value::from(kind));
    }
}

fn append_native(
    trace: &mut PhpArray,
    native: &crate::error_traces::NativeTrace,
    location: Option<(&str, usize)>,
    arguments: bool,
) {
    let mut entry = PhpArray::new();
    if let Some((file, line)) = location {
        entry.insert(ArrayKey::str("file"), Value::from(file));
        entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
    }
    entry.insert(ArrayKey::str("function"), Value::from(native.name.clone()));
    if arguments {
        entry.insert(ArrayKey::str("args"), native.arguments.clone());
    }
    let _ = trace.push(Value::from(entry));
}
