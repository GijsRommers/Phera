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
}
