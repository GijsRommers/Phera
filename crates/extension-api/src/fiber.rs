//! `$fiber->resume($message)` returns the message into its suspended call.

use std::rc::Rc;

use crate::{Object, PhpArray, Value};

pub enum FiberOperation {
    Construct(Value),
    Start(PhpArray),
    Resume(Option<Value>),
    Throw(Value),
}

pub enum FiberInspection {
    File,
    Line,
    Trace {
        options: i64,
        reflection: Rc<Object>,
        argument: Option<Value>,
    },
}

pub fn fiber_error(message: impl Into<String>) -> crate::NativeError {
    crate::NativeError::throw("FiberError", message)
}

pub(crate) fn unavailable() -> crate::NativeResult {
    Err(crate::NativeError::error(
        "This context has no PHP fiber scheduler",
    ))
}
