//! `debug_backtrace()` and `$exception->getTrace()` read the same heap frames.

use std::rc::Rc;

use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, calls::frame::FrameView};

mod arguments;

pub(crate) use arguments::{TraceArguments, materialize_trace_arguments, snapshot_arguments};

#[derive(Clone, Copy)]
pub(crate) struct TraceOptions {
    pub(crate) objects: bool,
    pub(crate) arguments: bool,
    pub(crate) limit: usize,
}

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
            if let Some((class, method, operator)) = native_method(&native.name) {
                entry.insert(ArrayKey::str("function"), Value::from(method));
                entry.insert(ArrayKey::str("class"), Value::from(class));
                entry.insert(ArrayKey::str("type"), Value::from(operator));
            } else {
                entry.insert(ArrayKey::str("function"), Value::from(native.name));
            }
            if !self.error_handlers.trace_ignore_args {
                entry.insert(ArrayKey::str("args"), native.arguments);
            }
            let _ = trace.push(Value::Array(Rc::new(entry)));
        }
        Value::from(self.capture_trace(trace, false, !self.error_handlers.trace_ignore_args, 0))
    }

    /// Stop at the requested number of PHP-visible frames, including native callbacks.
    pub(crate) fn capture_trace(
        &mut self,
        trace: PhpArray,
        objects: bool,
        arguments: bool,
        limit: i32,
    ) -> PhpArray {
        let limit = match limit {
            0 => usize::MAX,
            positive if positive > 0 => positive as usize,
            _ => return trace,
        };
        let options = TraceOptions {
            objects,
            arguments,
            limit,
        };
        let mut trace = self.capture_stack_trace(trace, options);
        if trace.len() < limit {
            self.append_fiber_caller(&mut trace, options);
        }
        trace
    }

    pub(crate) fn capture_stack_trace(
        &mut self,
        trace: PhpArray,
        options: TraceOptions,
    ) -> PhpArray {
        if self.generator_traces.is_empty() {
            self.trace_frames::<false>(trace, options)
        } else {
            self.trace_frames::<true>(trace, options)
        }
    }

    fn trace_frames<const GENERATORS: bool>(
        &mut self,
        mut trace: PhpArray,
        options: TraceOptions,
    ) -> PhpArray {
        let TraceOptions {
            objects,
            arguments,
            limit,
        } = options;
        let mut generators = if GENERATORS {
            self.generator_traces.len()
        } else {
            0
        };
        let mut callbacks = self.native_callback_sites.len();
        let stack_depth = self.call_stack.depth();
        let mut child_file = None;
        if callbacks != 0 {
            child_file =
                self.append_trace_callbacks(&mut trace, &mut callbacks, stack_depth, options);
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
            let callback = callbacks
                .checked_sub(1)
                .and_then(|index| self.native_callback_sites.get(index))
                .filter(|site| site.boundary == depth);
            let suspended_native = callee
                .extras
                .property_hook
                .as_deref()
                .and_then(crate::property_hook_continuation::HookContinuation::native_trace)
                .is_some();
            let internal = warning || callback.is_some() || suspended_native;
            let include_parent =
                depth > 1 && caller.is_some_and(|caller| caller.function_id().is_none());
            let mut entry = PhpArray::new();
            if let Some((file, line)) = callback.and_then(|site| site.native.child_location()) {
                add_source_location(&mut entry, file, line);
            } else if let Some(caller) = caller.filter(|_| !internal) {
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
            if GENERATORS
                && generators != 0
                && self.generator_traces[generators - 1].boundary == depth
            {
                generators -= 1;
                let caller = depth
                    .checked_sub(1)
                    .and_then(|index| self.frame_at(index))
                    .filter(|_| !internal)
                    .map(|frame| {
                        (
                            self.code.unit(frame.frame.function).file.as_str(),
                            frame.line(),
                        )
                    });
                self.generator_traces[generators].append_generator_trace(
                    &self.code,
                    &mut self.classes,
                    &mut trace,
                    entry,
                    caller,
                    options,
                );
            } else {
                let _ = trace.push(Value::from(entry));
            }
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
                    None,
                );
            } else if suspended_native
                && let Some(native) = self
                    .call_stack
                    .extras(depth)
                    .property_hook
                    .as_deref()
                    .and_then(crate::property_hook_continuation::HookContinuation::native_trace)
            {
                let caller = depth.checked_sub(1).and_then(|index| self.frame_at(index));
                let location = caller.map(|frame| (frame.unit().file.as_str(), frame.line()));
                append_native(
                    &mut trace,
                    &native.name,
                    location,
                    arguments.then(|| native.arguments.clone()),
                    None,
                );
                child_file = caller.map(|frame| frame.cache().trace_file(frame.unit()));
            }
            if callbacks != 0
                && let Some(file) =
                    self.append_trace_callbacks(&mut trace, &mut callbacks, depth, options)
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
        options: TraceOptions,
    ) -> Option<Value> {
        let mut last_file = None;
        while trace.len() < options.limit {
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
            if let Some(name) = site.native.name() {
                let location = caller.map(|caller| (caller.unit().file.as_str(), caller.line()));
                append_native(
                    trace,
                    name,
                    location,
                    options.arguments.then(|| site.native.arguments()),
                    options.objects.then(|| site.native.object()).flatten(),
                );
            }
            if let Some(caller) = caller {
                last_file = Some(caller.cache().trace_file(caller.unit()));
                break;
            }
        }
        last_file
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
    let scope = if function.starts_with("{closure:") {
        callee.scope().map(|scope| scope.name.as_str())
    } else {
        None
    };
    add_trace_function(entry, function, scope, callee.this(), objects);
}

/// Receiver and declaring scope are independent when a closure is bound.
pub(crate) fn add_trace_function(
    entry: &mut PhpArray,
    function: &str,
    scope: Option<&str>,
    object: Option<&Rc<phera_value::Object>>,
    objects: bool,
) {
    let (name, class) = if function.starts_with("{closure:") {
        (
            function,
            scope.or_else(|| object.map(|object| object.class.name())),
        )
    } else if let Some((class, name)) = function.split_once("::") {
        (name, Some(class))
    } else {
        (function, None)
    };
    entry.insert(ArrayKey::str("function"), Value::from(name));
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
    name: &str,
    location: Option<(&str, usize)>,
    arguments: Option<Value>,
    object: Option<&Rc<phera_value::Object>>,
) {
    let mut entry = PhpArray::new();
    if let Some((file, line)) = location {
        add_source_location(&mut entry, file, line);
    }
    if let Some((class, method, operator)) = native_method(name) {
        entry.insert(ArrayKey::str("function"), Value::from(method));
        entry.insert(ArrayKey::str("class"), Value::from(class));
        if let Some(object) = object {
            entry.insert(ArrayKey::str("object"), Value::Object(object.clone()));
        }
        entry.insert(ArrayKey::str("type"), Value::from(operator));
    } else {
        entry.insert(ArrayKey::str("function"), Value::from(name));
    }
    if let Some(arguments) = arguments {
        entry.insert(ArrayKey::str("args"), arguments);
    }
    let _ = trace.push(Value::from(entry));
}

fn native_method(name: &str) -> Option<(&str, &str, &str)> {
    if let Some((class, method)) = name.split_once("->") {
        Some((class, method, "->"))
    } else {
        name.split_once("::")
            .map(|(class, method)| (class, method, "::"))
    }
}

fn add_source_location(entry: &mut PhpArray, file: &str, line: usize) {
    entry.insert(ArrayKey::str("file"), Value::from(file));
    entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
}
