//! Delegation contributes the suspended callers to the PHP backtrace.
//!
//! ```php
//! function outer() { yield from inner(); }
//! outer()->current(); // inner(), outer(), Generator->current()
//! ```

use std::rc::Rc;

use phera_extension_api::{GeneratorOperation, NativeError, traversal::IteratorStep};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, error_traces::NativeTrace};

use super::{State, resume::Ancestors, state};

pub(crate) struct Site {
    pub(crate) boundary: usize,
    pub(super) ancestors: Ancestors,
    method: Option<Method>,
}

struct Method {
    name: &'static str,
    argument: Option<Value>,
}

impl Site {
    pub(crate) fn new(boundary: usize, operation: &GeneratorOperation) -> Self {
        let method = match operation {
            GeneratorOperation::Cursor { .. } => None,
            GeneratorOperation::Send(value) => Some(Method {
                name: "send",
                argument: Some(value.clone()),
            }),
            GeneratorOperation::Throw(value) => Some(Method {
                name: "throw",
                argument: Some(value.clone()),
            }),
            GeneratorOperation::GetReturn => Some(Method {
                name: "getReturn",
                argument: None,
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
            }),
        };
        Self {
            boundary,
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
            &format!("Generator::{}", self.name),
            self.argument.as_slice(),
        )
    }
}

impl Vm {
    /// Only a failing or explicitly inspected generator needs to read the suspended windows.
    pub(crate) fn append_generator_trace(
        &self,
        site: &Site,
        trace: &mut PhpArray,
        mut entry: PhpArray,
        caller: (&str, usize),
        via_callback: bool,
    ) {
        for ancestor in site.ancestors.iter().rev() {
            let frame = state(ancestor, |generator| {
                let State::Delegating(activation, ..) = &generator.state else {
                    return None;
                };
                let frame = activation.frame;
                let function = self.code.function(frame.function);
                let file = &self.code.unit(frame.function).file;
                let line = function
                    .lines
                    .get(frame.ip().saturating_sub(1))
                    .copied()
                    .unwrap_or(1);
                let mut suspended = PhpArray::new();
                let name = &function.name;
                let (class, name) = match name.split_once("::") {
                    Some((class, name)) if !class.starts_with("{closure:") => (Some(class), name),
                    _ => (None, name.as_str()),
                };
                place(&mut entry, Some((file, line as usize)));
                suspended.insert(ArrayKey::str("function"), Value::from(name));
                if let Some(class) = class {
                    suspended.insert(ArrayKey::str("class"), Value::from(class));
                    suspended.insert(
                        ArrayKey::str("type"),
                        Value::from(if activation.has_receiver() {
                            "->"
                        } else {
                            "::"
                        }),
                    );
                }
                suspended.insert(ArrayKey::str("args"), activation.trace_arguments(function));
                Some(suspended)
            });
            if let Ok(Some(suspended)) = frame {
                let _ = trace.push(Value::Array(Rc::new(entry)));
                entry = suspended;
            }
        }
        place(
            &mut entry,
            (site.method.is_none() && !via_callback).then_some(caller),
        );
        let _ = trace.push(Value::Array(Rc::new(entry)));
        if let Some(method) = &site.method {
            let mut entry = PhpArray::new();
            place(&mut entry, (!via_callback).then_some(caller));
            entry.insert(ArrayKey::str("class"), Value::from("Generator"));
            entry.insert(ArrayKey::str("function"), Value::from(method.name));
            entry.insert(ArrayKey::str("type"), Value::from("->"));
            entry.insert(ArrayKey::str("args"), method.native_trace().arguments);
            let _ = trace.push(Value::Array(Rc::new(entry)));
        }
    }
}

fn place(entry: &mut PhpArray, site: Option<(&str, usize)>) {
    if let Some((file, line)) = site {
        entry.insert(ArrayKey::str("file"), Value::from(file));
        entry.insert(ArrayKey::str("line"), Value::Int(line as i64));
    } else {
        entry.remove(&ArrayKey::str("file"));
        entry.remove(&ArrayKey::str("line"));
    }
}
