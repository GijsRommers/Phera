//! An exception from `$fiber->resume()` retains the fiber's PHP frames and
//! the method that entered that stack. Reflection omits the outer caller.

use phera_extension_api::{NativeError, NativeResult, fiber::FiberInspection};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, exceptions::TraceOrigin};

#[derive(Clone, Copy)]
pub(super) enum Method {
    Start,
    Resume,
    Throw,
}

impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Resume => "resume",
            Self::Throw => "throw",
        }
    }
}

pub(super) struct Site {
    method: Method,
    file: Value,
    line: i64,
    arguments: Option<Value>,
    parent: Value,
    pub(super) park_caller: bool,
}

impl Site {
    pub(super) fn from_caller(vm: &mut Vm, method: Method, arguments: &PhpArray) -> Self {
        let (file, line) = vm.exception_location();
        let parent = if vm.active_fiber.is_some() {
            vm.throwable_trace(TraceOrigin::Construction)
        } else {
            Value::Null
        };
        Self {
            method,
            file,
            line: line as i64,
            arguments: (!arguments.is_empty()).then(|| Value::from(arguments.clone())),
            parent,
            park_caller: vm.active_fiber.is_none() && vm.call_stack.depth() > 1,
        }
    }

    pub(super) fn with_value(
        vm: &mut Vm,
        method: Method,
        value: Option<&Value>,
    ) -> Result<Self, NativeError> {
        let mut arguments = PhpArray::new();
        if let Some(value) = value {
            arguments.push(value.clone())?;
        }
        Ok(Self::from_caller(vm, method, &arguments))
    }

    pub(super) fn trace_entry(&self, object: Option<Value>, ignore_arguments: bool) -> PhpArray {
        let mut entry = new_entry(
            Some((self.file.clone(), self.line)),
            "Fiber",
            self.method.name(),
            "->",
            object,
        );
        if !ignore_arguments {
            let args = self
                .arguments
                .clone()
                .unwrap_or_else(|| Value::from(PhpArray::new()));
            entry.insert(ArrayKey::str("args"), args);
        }
        entry
    }

    fn append(&self, trace: &mut PhpArray, ignore_arguments: bool) {
        let _ = trace.push(Value::from(self.trace_entry(None, ignore_arguments)));
        if let Value::Array(parent) = &self.parent {
            for (_, entry) in parent.iter() {
                let _ = trace.push(entry.clone());
            }
        }
    }
}

pub(super) enum TraceEntry<'a> {
    Reflection,
    Suspend(Option<&'a Value>),
}

pub(super) struct Waiting {
    location: Option<(Value, i64)>,
    entry: PhpArray,
    frames: Value,
}

impl Waiting {
    pub(super) fn capture(vm: &mut Vm, entry: PhpArray) -> Self {
        Self {
            location: vm.fiber_location(),
            entry,
            frames: vm.trace_without_fiber_caller(true),
        }
    }

    pub(super) fn inspect(&self, inspection: FiberInspection) -> NativeResult {
        match inspection {
            FiberInspection::File => Ok(self
                .location
                .as_ref()
                .map_or(Value::Null, |(file, _)| file.clone())),
            FiberInspection::Line => Ok(self
                .location
                .as_ref()
                .map_or(Value::Null, |(_, line)| Value::Int(*line))),
            FiberInspection::Trace { options, .. } => {
                combine(self.entry.clone(), &self.frames, options)
            }
        }
    }
}

fn new_entry(
    location: Option<(Value, i64)>,
    class: &'static str,
    method: &'static str,
    access: &'static str,
    object: Option<Value>,
) -> PhpArray {
    let mut entry = PhpArray::new();
    if let Some((file, line)) = location {
        entry.insert(ArrayKey::str("file"), file);
        entry.insert(ArrayKey::str("line"), Value::Int(line));
    }
    entry.insert(ArrayKey::str("function"), Value::from(method));
    entry.insert(ArrayKey::str("class"), Value::from(class));
    if let Some(object) = object {
        entry.insert(ArrayKey::str("object"), object);
    }
    entry.insert(ArrayKey::str("type"), Value::from(access));
    entry
}

fn filter(entry: &mut PhpArray, options: i64) {
    if options & 1 == 0 {
        entry.remove(&ArrayKey::str("object"));
    }
    if options & 2 != 0 {
        entry.remove(&ArrayKey::str("args"));
    }
}

fn combine(mut entry: PhpArray, frames: &Value, options: i64) -> NativeResult {
    let mut trace = PhpArray::new();
    filter(&mut entry, options);
    trace.push(Value::from(entry))?;
    if let Value::Array(frames) = frames {
        for (_, frame) in frames.iter() {
            let value = if let Value::Array(entry) = frame {
                let mut entry = entry.as_ref().clone();
                filter(&mut entry, options);
                Value::from(entry)
            } else {
                frame.clone()
            };
            trace.push(value)?;
        }
    }
    Ok(Value::from(trace))
}

impl Vm {
    /// Only exception construction examines the active fiber's caller metadata.
    pub(crate) fn append_fiber_caller(&mut self, trace: &mut PhpArray) {
        let Some(exchange) = self
            .active_fiber
            .as_ref()
            .map(|active| active.exchange.clone())
        else {
            return;
        };
        let site = exchange.site.borrow();
        if let Some(site) = site.as_ref() {
            site.append(trace, self.error_handlers.trace_ignore_args);
            if site.park_caller
                && let Some(caller) = exchange.caller.borrow_mut().as_mut()
                && let Value::Array(parent) = caller.trace(self)
            {
                for (_, entry) in parent.iter() {
                    let _ = trace.push(entry.clone());
                }
            }
        }
    }

    fn fiber_location(&self) -> Option<(Value, i64)> {
        self.current()
            .map(|frame| (frame.cache().trace_file(frame.unit()), frame.line() as i64))
    }

    pub(super) fn fiber_snapshot(
        &mut self,
        inspection: FiberInspection,
        first: TraceEntry<'_>,
    ) -> NativeResult {
        let location = self.fiber_location();
        match inspection {
            FiberInspection::File => Ok(location.map_or(Value::Null, |(file, _)| file)),
            FiberInspection::Line => Ok(location.map_or(Value::Null, |(_, line)| Value::Int(line))),
            FiberInspection::Trace {
                options,
                reflection,
                argument,
            } => {
                let (mut entry, argument) = match first {
                    TraceEntry::Reflection => (
                        new_entry(
                            location,
                            "ReflectionFiber",
                            "getTrace",
                            "->",
                            Some(Value::Object(reflection)),
                        ),
                        argument.as_ref(),
                    ),
                    TraceEntry::Suspend(value) => {
                        (new_entry(location, "Fiber", "suspend", "::", None), value)
                    }
                };
                if options & 2 == 0 {
                    let mut args = PhpArray::new();
                    if let Some(argument) = argument {
                        args.push(argument.clone())?;
                    }
                    entry.insert(ArrayKey::str("args"), Value::from(args));
                }
                let frames = self.trace_without_fiber_caller(options & 1 != 0);
                combine(entry, &frames, options)
            }
        }
    }
}
