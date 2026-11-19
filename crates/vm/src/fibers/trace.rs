//! An exception from `$fiber->resume()` retains the fiber's PHP frames and
//! the method that entered that stack. Reflection can omit the caller.

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
}

impl Site {
    pub(super) fn from_caller(vm: &mut Vm, method: Method, arguments: &PhpArray) -> Self {
        let (file, line) = vm.exception_location();
        let parent = if vm.call_stack.depth() > 1 || vm.active_fiber.is_some() {
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
        }
    }

    pub(super) fn with_value(
        vm: &mut Vm,
        method: Method,
        value: &Value,
    ) -> Result<Self, NativeError> {
        let mut arguments = PhpArray::new();
        if !matches!(value, Value::Null) {
            arguments.push(value.clone())?;
        }
        Ok(Self::from_caller(vm, method, &arguments))
    }

    fn append(&self, trace: &mut PhpArray, ignore_arguments: bool) {
        let mut entry = PhpArray::new();
        entry.insert(ArrayKey::str("file"), self.file.clone());
        entry.insert(ArrayKey::str("line"), Value::Int(self.line));
        entry.insert(ArrayKey::str("function"), Value::from(self.method.name()));
        entry.insert(ArrayKey::str("class"), Value::from("Fiber"));
        entry.insert(ArrayKey::str("type"), Value::from("->"));
        if !ignore_arguments {
            let args = self
                .arguments
                .clone()
                .unwrap_or_else(|| Value::from(PhpArray::new()));
            entry.insert(ArrayKey::str("args"), args);
        }
        let _ = trace.push(Value::from(entry));
        if let Value::Array(parent) = &self.parent {
            for (_, entry) in parent.iter() {
                let _ = trace.push(entry.clone());
            }
        }
    }
}

impl Vm {
    /// Only exception construction examines the active fiber's caller metadata.
    pub(crate) fn append_fiber_caller(&self, trace: &mut PhpArray) {
        let site = self
            .active_fiber
            .as_ref()
            .and_then(|active| active.exchange.site.try_borrow().ok());
        if let Some(site) = site.as_deref().and_then(Option::as_ref) {
            site.append(trace, self.error_handlers.trace_ignore_args);
        }
    }

    pub(super) fn fiber_snapshot(
        &mut self,
        inspection: FiberInspection,
        suspension: Option<&Value>,
    ) -> NativeResult {
        let current = self.current();
        match inspection {
            FiberInspection::File => {
                Ok(current.map_or(Value::Null, |frame| frame.cache().trace_file(frame.unit())))
            }
            FiberInspection::Line => {
                Ok(current.map_or(Value::Null, |frame| Value::Int(frame.line() as i64)))
            }
            FiberInspection::Trace(options) => {
                let mut trace = PhpArray::new();
                if let Some(value) = suspension {
                    let (file, line) = self.exception_location();
                    let mut entry = PhpArray::new();
                    entry.insert(ArrayKey::str("file"), file);
                    entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
                    entry.insert(ArrayKey::str("function"), Value::from("suspend"));
                    entry.insert(ArrayKey::str("class"), Value::from("Fiber"));
                    entry.insert(ArrayKey::str("type"), Value::from("::"));
                    if options & 2 == 0 {
                        let mut arguments = PhpArray::new();
                        if !matches!(value, Value::Null) {
                            arguments.push(value.clone())?;
                        }
                        entry.insert(ArrayKey::str("args"), Value::from(arguments));
                    }
                    trace.push(Value::from(entry))?;
                }
                let Value::Array(array) = self.trace_without_fiber_caller() else {
                    return Ok(Value::from(trace));
                };
                for (_, value) in array.iter() {
                    let value = if options & 2 != 0
                        && let Value::Array(entry) = value
                    {
                        let mut entry = entry.as_ref().clone();
                        entry.remove(&ArrayKey::str("args"));
                        Value::from(entry)
                    } else {
                        value.clone()
                    };
                    trace.push(value)?;
                }
                Ok(Value::from(trace))
            }
        }
    }
}
