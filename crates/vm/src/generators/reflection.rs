//! Reflection reads a suspended or installed activation without executing it.
//!
//! ```php
//! $ref = new ReflectionGenerator($source);
//! $ref->getExecutingGenerator(); // the active leaf of a yield-from chain
//! ```

use std::{collections::HashSet, rc::Rc};

use phera_extension_api::reflection::{FunctionTarget, GeneratorQuery};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::{Vm, code_table::FunctionIndex, exception_traces::TraceArguments};

use super::{State, activation::Activation, delegation::Delegate, state};

struct TraceFrame {
    function: FunctionIndex,
    line: usize,
    receiver: Option<Rc<Object>>,
    arguments: Option<TraceArguments>,
}

impl Vm {
    pub(crate) fn generator_function(
        &self,
        object: &Object,
    ) -> Result<FunctionTarget, NativeError> {
        state(object, |generator| {
            if let Some(closure) = &generator.closure {
                return FunctionTarget::Closure(Rc::downgrade(closure));
            }
            let name = &self.code.function(generator.function).name;
            if let Some((class, method)) = name.rsplit_once("::") {
                FunctionTarget::Method {
                    class: class.into(),
                    name: method.into(),
                }
            } else {
                FunctionTarget::Named(name.clone())
            }
        })
    }

    pub(crate) fn inspect_generator(
        &mut self,
        object: &Rc<Object>,
        query: GeneratorQuery,
    ) -> NativeResult {
        match query {
            GeneratorQuery::Closed => state(object, is_closed).map(Value::Bool),
            GeneratorQuery::Executing => {
                ensure_open(object)?;
                let mut leaf = object.clone();
                let mut visited = HashSet::new();
                while let Some(delegate) = open_delegate(&leaf)? {
                    if !visited.insert(Rc::as_ptr(&leaf)) {
                        return Err(internal_error!("cyclic generator delegate"));
                    }
                    leaf = delegate;
                }
                Ok(Value::Object(leaf))
            }
            GeneratorQuery::Trace(options) => self.generator_reflection_trace(object, options),
            query => {
                let frame = self.generator_reflection_frame(object, false)?;
                Ok(match query {
                    GeneratorQuery::File => {
                        Value::from(self.code.unit(frame.function).file.as_str())
                    }
                    GeneratorQuery::Line => Value::Int(frame.line as i64),
                    GeneratorQuery::This => frame.receiver.map_or(Value::Null, Value::Object),
                    _ => return Err(internal_error!("generator frame query")),
                })
            }
        }
    }

    fn generator_reflection_frame(
        &self,
        object: &Object,
        arguments: bool,
    ) -> Result<TraceFrame, NativeError> {
        state(object, |generator| match &generator.state {
            State::Created(frame)
            | State::Ready(frame)
            | State::Yielded(frame, _)
            | State::Delegating(frame, ..) => Ok(self.suspended_reflection_frame(frame, arguments)),
            State::Running => {
                let depth = self
                    .generator_traces
                    .iter()
                    .rev()
                    .find_map(|site| {
                        (site.active == Some(std::ptr::from_ref(object))).then_some(site.boundary)
                    })
                    .ok_or_else(|| internal_error!("running generator reflection frame"))?;
                let frame = self
                    .frame_at(depth)
                    .ok_or_else(|| internal_error!("running generator reflection frame"))?;
                Ok(TraceFrame {
                    function: frame.frame.function,
                    line: frame.line(),
                    receiver: frame.this().cloned(),
                    arguments: arguments.then(|| self.frame_trace_arguments(frame)),
                })
            }
            State::Returned(_) | State::Failed => Err(closed()),
        })?
    }

    fn suspended_reflection_frame(&self, activation: &Activation, arguments: bool) -> TraceFrame {
        let function = activation.frame.function;
        let body = self.code.function(function);
        let line = body
            .lines
            .get(activation.frame.ip().saturating_sub(1))
            .copied()
            .unwrap_or(body.definition_line);
        TraceFrame {
            function,
            line: line as usize,
            receiver: activation.receiver().cloned(),
            arguments: arguments.then(|| activation.trace_arguments(&self.code)),
        }
    }

    fn generator_reflection_trace(&mut self, object: &Rc<Object>, options: i64) -> NativeResult {
        let mut current = object.clone();
        let mut visited = HashSet::new();
        let mut frames = Vec::new();
        loop {
            if !visited.insert(Rc::as_ptr(&current)) {
                return Err(internal_error!("cyclic generator delegate"));
            }
            let frame = self.generator_reflection_frame(&current, options & 2 == 0)?;
            let closure_scope = state(&current, |generator| {
                generator.closure.as_ref().and_then(closure_scope)
            })?;
            frames.push((frame, closure_scope));
            let Some(delegate) = open_delegate(&current)? else {
                break;
            };
            current = delegate;
        }

        let mut output = PhpArray::new();
        while let Some((frame, closure_scope)) = frames.pop() {
            let mut entry = PhpArray::new();
            if let Some((caller, _)) = frames.last() {
                entry.insert(
                    ArrayKey::str("file"),
                    Value::from(self.code.unit(caller.function).file.as_str()),
                );
                entry.insert(ArrayKey::str("line"), Value::Int(caller.line as i64));
            }
            let fullname = &self.code.function(frame.function).name;
            let (name, scope) = if fullname.starts_with("{closure:") {
                (fullname.as_str(), closure_scope.as_deref())
            } else if let Some((scope, name)) = fullname.rsplit_once("::") {
                (name, Some(scope))
            } else {
                (fullname.as_str(), None)
            };
            entry.insert(ArrayKey::str("function"), Value::from(name));
            if let Some(scope) = scope {
                entry.insert(ArrayKey::str("class"), Value::from(scope));
                if options & 1 != 0
                    && let Some(receiver) = &frame.receiver
                {
                    entry.insert(ArrayKey::str("object"), Value::Object(receiver.clone()));
                }
                entry.insert(
                    ArrayKey::str("type"),
                    Value::from(if frame.receiver.is_some() { "->" } else { "::" }),
                );
            }
            if let Some(arguments) = frame.arguments {
                entry.insert(
                    ArrayKey::str("args"),
                    crate::exception_traces::materialize_trace_arguments(
                        &mut self.classes,
                        arguments,
                    ),
                );
            }
            output.push(Value::from(entry))?;
        }
        Ok(Value::from(output))
    }
}

pub(super) fn closure_scope(closure: &Rc<Object>) -> Option<String> {
    let native = closure.native.try_borrow().ok()?;
    let closure = native
        .as_ref()?
        .downcast_ref::<crate::closures::ClosureState>()?;
    closure
        .context
        .as_ref()
        .map(|context| context.scope.name.clone())
}

fn open_delegate(object: &Object) -> Result<Option<Rc<Object>>, NativeError> {
    let delegate = state(object, |generator| match &generator.state {
        State::Delegating(_, _, Delegate::Generator(child)) => Some(child.clone()),
        _ => None,
    })?;
    match delegate {
        Some(child) if !state(&child, is_closed)? => Ok(Some(child)),
        _ => Ok(None),
    }
}

fn ensure_open(object: &Object) -> Result<(), NativeError> {
    if state(object, is_closed)? {
        Err(closed())
    } else {
        Ok(())
    }
}

fn is_closed(generator: &mut super::Generator) -> bool {
    matches!(generator.state, State::Returned(_) | State::Failed)
}

fn closed() -> NativeError {
    NativeError::throw(
        "ReflectionException",
        "Cannot fetch information from a closed Generator",
    )
}
