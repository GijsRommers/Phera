use std::any::Any;
use std::fmt;

use phera_value::{ArrayKey, OpError, PhpArray, Value};

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
    /// Rich native throw requests also use this transport until the VM creates their object.
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
    /// Preserve a driver's code without coercion: `$e->getCode()` can return `"42P01"`.
    /// Only integers and strings are valid; other values report an extension implementation error.
    /// An exception already unwinding from PHP is passed through unchanged.
    pub fn with_code(self, code: impl Into<Value>) -> Self {
        match self.into_exception_parts() {
            Ok((class, message, _, properties)) => {
                let code = code.into();
                if !matches!(code, Value::Int(_) | Value::String(_)) {
                    return Self::error(
                        "Phera internal error: native exception code must be int or string",
                    );
                }
                Self::Unwind(Unwind(Box::new(ExceptionPayload {
                    class,
                    message,
                    code,
                    properties,
                })))
            }
            Err(error) => error,
        }
    }

    /// Populate a declared property before throwing, for example `$e->errorInfo = ['42P01']`.
    /// The VM checks the declaration so a misspelled name cannot silently add a dynamic property.
    /// Repeating a name replaces its value, as a PHP assignment would.
    pub fn with_property(self, name: &str, value: Value) -> Self {
        match self.into_exception_parts() {
            Ok((class, message, code, mut properties)) => {
                properties.insert(ArrayKey::str(name), value);
                Self::Unwind(Unwind(Box::new(ExceptionPayload {
                    class,
                    message,
                    code,
                    properties,
                })))
            }
            Err(error) => error,
        }
    }

    /// Consume a native throw request so a runtime or test can create its PHP exception object.
    /// PHP's `catch (PDOException $e) { echo $e->getCode(); }` observes the supplied string code.
    /// A PDO failure supplies `"PDOException"`, a message, `"42P01"`, and `errorInfo`.
    /// Existing PHP unwinds are returned intact; their object and call-site trace must survive.
    pub fn into_exception_parts(self) -> Result<(&'static str, String, Value, PhpArray), Self> {
        match self {
            Self::Throw {
                class,
                message,
                code,
            } => Ok((class, message, Value::Int(code), PhpArray::new())),
            Self::Unwind(payload) => match payload.0.downcast::<ExceptionPayload>() {
                Ok(exception) => Ok((
                    exception.class,
                    exception.message,
                    exception.code,
                    exception.properties,
                )),
                Err(payload) => Err(Self::Unwind(Unwind(payload))),
            },
        }
    }

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

/// Rich native requests use the existing opaque transport, preserving exhaustive enum callers.
/// Only the runtime materializes these; callbacks must forward them just like a PHP unwind.
#[derive(Debug)]
struct ExceptionPayload {
    class: &'static str,
    message: String,
    code: Value,
    properties: PhpArray,
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
