//! Operations emitted instead of an ordinary call, such as `strlen($text)`.

use crate::TypeMask;

/// The unary builtin forms that Zend compiles independently of function calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinUnary {
    Strlen,
    Count,
    Sizeof,
    TypeCheck(TypeMask),
    IsResource,
    GetType,
    GetClass,
    Defined,
}

/// Builtins that take no argument registers, such as `func_num_args()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinFrame {
    ArgumentCount,
    ArgumentsFrom(u32),
    CalledClass,
    AssertionsDisabled,
}

/// `call_user_func($callback, $value)` and `call_user_func_array($callback, $values)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackArguments {
    Positional,
    Array,
}

impl CallbackArguments {
    pub const fn function(self) -> &'static str {
        match self {
            Self::Positional => "call_user_func",
            Self::Array => "call_user_func_array",
        }
    }
}
