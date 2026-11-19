//! Delegation contributes the suspended callers to the PHP backtrace.
//!
//! ```php
//! function outer() { yield from inner(); }
//! outer()->current(); // inner(), outer(), Generator->current()
//! ```

use std::rc::Rc;

use phera_extension_api::{GeneratorOperation, NativeError, traversal::IteratorStep};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::{
    classes::ClassTable,
    code_table::CodeTable,
    error_traces::NativeTrace,
    exception_traces::{TraceOptions, add_trace_function, materialize_trace_arguments},
};

use super::{State, resume::Ancestors, state};

pub(crate) struct Site {
    pub(crate) boundary: usize,
    pub(super) active: Option<*const Object>,
    pub(super) ancestors: Ancestors,
    method: Option<Method>,
}

struct Method {
    name: &'static str,
    argument: Option<Value>,
    receiver: Rc<Object>,
}

impl Site {
    pub(crate) fn new(
        boundary: usize,
        operation: &GeneratorOperation,
        receiver: &Rc<Object>,
    ) -> Self {
        let method = match operation {
            GeneratorOperation::Cursor { .. } => None,
            GeneratorOperation::Send(value) => Some(Method {
                name: "send",
                argument: Some(value.clone()),
                receiver: receiver.clone(),
            }),
            GeneratorOperation::Throw(value) => Some(Method {
                name: "throw",
                argument: Some(value.clone()),
                receiver: receiver.clone(),
            }),
            GeneratorOperation::GetReturn => Some(Method {
                name: "getReturn",
                argument: None,
                receiver: receiver.clone(),
            }),
            GeneratorOperation::Method(step) => Some(Method {
                name: match step {
                    IteratorStep::Rewind => "rewind",
                    IteratorStep::Advance => "next",
                    IteratorStep::Valid => "valid",
                    IteratorStep::Current => "current",
                    IteratorStep::Key => "key",
                },
                argument: None,
                receiver: receiver.clone(),
            }),
        };
        Self {
            boundary,
            active: None,
            ancestors: Ancestors::new(),
            method,
        }
    }

    pub(crate) fn failed_call(self, result: &Result<Value, NativeError>) -> Option<NativeTrace> {
        match result {
            Err(NativeError::Throw { .. }) => self.method.map(|method| method.native_trace()),
            _ => None,
        }
    }
}

impl Method {
    fn native_trace(&self) -> NativeTrace {
        NativeTrace::new(
            &format!("Generator->{}", self.name),
            self.argument.as_slice(),
        )
    }
}

impl Site {
    /// Suspended callers are visited only until the requested visible frame count.
    pub(crate) fn append_generator_trace(
        &self,
        code: &CodeTable,
        classes: &mut ClassTable,
        trace: &mut PhpArray,
        mut entry: PhpArray,
        caller: Option<(&str, usize)>,
        options: TraceOptions,
    ) {
        for ancestor in self.ancestors.iter().rev() {
            let last = trace.len().saturating_add(1) >= options.limit;
            let suspended = state(ancestor, |generator| {
                let State::Delegating(activation, ..) = &generator.state else {
                    return None;
                };
                let frame = activation.frame;
                let function = code.function(frame.function);
                let file = &code.unit(frame.function).file;
                let line = function
                    .lines
                    .get(frame.ip().saturating_sub(1))
                    .copied()
                    .unwrap_or(1);
                place(&mut entry, Some((file, line as usize)));
                Some((!last).then(|| {
                    let mut suspended = PhpArray::new();
                    let scope = generator
                        .closure
                        .as_ref()
                        .and_then(super::reflection::closure_scope);
                    add_trace_function(
                        &mut suspended,
                        code.function_name(frame.function),
                        scope.as_deref(),
                        activation.receiver(),
                        options.objects,
                    );
                    let arguments = options.arguments.then(|| activation.trace_arguments(code));
                    (suspended, arguments)
                }))
            });
            if let Ok(Some(next)) = suspended {
                let _ = trace.push(Value::from(entry));
                let Some((mut suspended, arguments)) = next else {
                    return;
                };
                if let Some(arguments) = arguments {
                    suspended.insert(
                        ArrayKey::str("args"),
                        materialize_trace_arguments(classes, arguments),
                    );
                }
                entry = suspended;
            }
        }
        place(
            &mut entry,
            if self.method.is_none() { caller } else { None },
        );
        let _ = trace.push(Value::from(entry));
        if trace.len() < options.limit
            && let Some(method) = &self.method
        {
            let mut entry = PhpArray::new();
            place(&mut entry, caller);
            entry.insert(ArrayKey::str("function"), Value::from(method.name));
            entry.insert(ArrayKey::str("class"), Value::from("Generator"));
            if options.objects {
                entry.insert(
                    ArrayKey::str("object"),
                    Value::Object(method.receiver.clone()),
                );
            }
            entry.insert(ArrayKey::str("type"), Value::from("->"));
            if options.arguments {
                entry.insert(ArrayKey::str("args"), method.native_trace().arguments);
            }
            let _ = trace.push(Value::from(entry));
        }
    }
}

fn place(entry: &mut PhpArray, site: Option<(&str, usize)>) {
    let file_key = ArrayKey::str("file");
    let line_key = ArrayKey::str("line");
    let Some((file, line)) = site else {
        entry.remove(&file_key);
        entry.remove(&line_key);
        return;
    };
    if !entry.is_empty() && !entry.contains_key(&file_key) {
        let mut located = PhpArray::new();
        located.insert(file_key, Value::from(file));
        located.insert(line_key, Value::Int(line as i64));
        for (key, value) in entry.iter() {
            located.insert(key, value.clone());
        }
        *entry = located;
    } else {
        entry.insert(file_key, Value::from(file));
        entry.insert(line_key, Value::Int(line as i64));
    }
}
