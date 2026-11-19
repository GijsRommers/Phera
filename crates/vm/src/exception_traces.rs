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
        let stack_depth = self.call_stack.depth();
        let mut child_file = None;
        if callbacks != 0 {
            child_file = self.append_trace_callbacks(
                &mut trace,
                &mut callbacks,
                stack_depth,
                limit,
                arguments,
            );
        }
        for depth in (0..stack_depth).rev() {
            if trace.len() >= limit {
                break;
            }
            let Some(callee) = self.frame_at(depth) else {
                break;
            };
            let caller = depth.checked_sub(1).and_then(|index| self.frame_at(index));
            let function = callee.function_id().is_some();
            if !function && caller.is_none() {
                break;
            }
            let warning = self
                .error_handlers
                .warning_site
                .as_ref()
                .is_some_and(|site| site.boundary == depth);
            let callback = callbacks != 0
                && self
                    .native_callback_sites
                    .get(callbacks - 1)
                    .is_some_and(|site| site.boundary == depth);
            let internal = warning || callback;
            let include_parent =
                depth > 1 && caller.is_some_and(|caller| caller.function_id().is_none());
            let mut entry = PhpArray::new();
            if let Some(caller) = caller.filter(|_| !internal) {
                add_location(&mut entry, caller);
            }
            if function {
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
            child_file = if include_parent {
                entry.get(&ArrayKey::str("file")).cloned()
            } else {
                None
            };
            let _ = trace.push(Value::from(entry));
            if !internal {
                continue;
            }
            if trace.len() >= limit {
                break;
            }
            if warning && let Some(site) = self.error_handlers.warning_site.as_ref() {
                append_native(
                    &mut trace,
                    &site.native.name,
                    Some((&site.location.file, site.location.line)),
                    arguments.then(|| site.native.arguments.clone()),
                );
            }
            if callbacks != 0
                && let Some(file) =
                    self.append_trace_callbacks(&mut trace, &mut callbacks, depth, limit, arguments)
            {
                child_file = Some(file);
            }
        }
        trace
    }

    /// A native operation can call another native function without pushing any PHP frame.
    fn append_trace_callbacks(
        &self,
        trace: &mut PhpArray,
        callbacks: &mut usize,
        boundary: usize,
        limit: usize,
        arguments: bool,
    ) -> Option<Value> {
        let mut last_file = None;
        while trace.len() < limit {
            let Some(index) = callbacks.checked_sub(1) else {
                break;
            };
            let Some(site) = self
                .native_callback_sites
                .get(index)
                .filter(|site| site.boundary == boundary)
            else {
                break;
            };
            *callbacks = index;
            let nested = index
                .checked_sub(1)
                .and_then(|parent| self.native_callback_sites.get(parent))
                .is_some_and(|parent| parent.boundary == boundary);
            let caller = if nested {
                None
            } else {
                boundary
                    .checked_sub(1)
                    .and_then(|parent| self.frame_at(parent))
            };
            let location = caller.map(|caller| (caller.unit().file.as_str(), caller.line()));
            append_native(
                trace,
                site.native.name(),
                location,
                arguments.then(|| site.native.arguments()),
            );
            if let Some(caller) = caller {
                last_file = Some(caller.cache().trace_file(caller.unit()));
                break;
            }
        }
        last_file
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
        let variadic = params.last().is_some_and(|parameter| parameter.variadic);
        let fixed = params.len() - usize::from(variadic);
        let count = if variadic {
            frame.argument_count()
        } else {
            frame.supplied_arguments()
        };
        for index in 0..fixed.min(count) {
            let value = frame
                .slots(&self.registers)
                .get(index)
                .and_then(Option::as_ref)
                .map_or(Value::Null, |value| value.deref().clone());
            let sensitive = crate::sensitive_parameters::is_sensitive(frame, index);
            snapshot.push((None, value, sensitive));
        }
        if let Some(items) = &frame.extras.variadic {
            let sensitive = crate::sensitive_parameters::is_sensitive(frame, fixed);
            for (key, value) in items.iter() {
                let name = match key {
                    ArrayKey::String(_) => Some(key),
                    _ => None,
                };
                snapshot.push((name, value.deref().clone(), sensitive));
            }
        } else if !variadic {
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
    let function = callee.code.function_name(callee.frame.function);
    let (name, class) = if function.starts_with("{closure:") {
        let class = callee.scope().map(|scope| scope.name.as_str());
        (
            function,
            class.or_else(|| callee.this().map(|object| object.class.name())),
        )
    } else if let Some((class, name)) = function.split_once("::") {
        (name, Some(class))
    } else {
        (function, None)
    };
    entry.insert(ArrayKey::str("function"), Value::from(name));
    if let Some(class) = class {
        let object = callee.this();
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
    name: &str,
    location: Option<(&str, usize)>,
    arguments: Option<Value>,
) {
    let mut entry = PhpArray::new();
    if let Some((file, line)) = location {
        entry.insert(ArrayKey::str("file"), Value::from(file));
        entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
    }
    entry.insert(ArrayKey::str("function"), Value::from(name));
    if let Some(arguments) = arguments {
        entry.insert(ArrayKey::str("args"), arguments);
    }
    let _ = trace.push(Value::from(entry));
}
