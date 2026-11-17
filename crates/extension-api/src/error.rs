use std::any::Any;
use std::fmt;

use phera_value::{OpError, Value};

pub type NativeResult = Result<Value, NativeError>;

/// Why a native function did not return a value.
#[derive(Debug)]
pub enum NativeError {
    /// Throw a new instance of a built-in PHP class, such as `ValueError`.
    /// The runtime creates the object; native code only names the class.
    Throw {
        class: &'static str,
        message: String,
        /// What `getCode()` returns; 0 unless the function sets one.
        code: i64,
    },
    /// The runtime is already unwinding: a callback threw, or called `exit()`.
    /// Native code passes this on untouched.
    Unwind(Unwind),
}

/// Opaque unwinding state owned by the runtime.
pub struct Unwind(pub Box<dyn Any>);

impl fmt::Debug for Unwind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Unwind(..)")
    }
}

impl NativeError {
    pub fn throw(class: &'static str, message: impl Into<String>) -> Self {
        NativeError::Throw {
            class,
            message: message.into(),
            code: 0,
        }
    }

    /// Like [`NativeError::throw`], for exceptions whose code matters, such
    /// as `JsonException`.
    pub fn throw_with_code(class: &'static str, message: impl Into<String>, code: i64) -> Self {
        NativeError::Throw {
            class,
            message: message.into(),
            code,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::throw("Error", message)
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self::throw("TypeError", message)
    }

    pub fn value_error(message: impl Into<String>) -> Self {
        Self::throw("ValueError", message)
    }

    pub fn argument_count_error(message: impl Into<String>) -> Self {
        Self::throw("ArgumentCountError", message)
    }
}

impl From<OpError> for NativeError {
    fn from(error: OpError) -> Self {
        NativeError::Throw {
            class: error.kind.class_name(),
            message: error.message,
            code: 0,
        }
    }
}
