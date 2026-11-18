//! PHP operators dispatched without heap-owned instructions.

use phera_bytecode::{BinaryOp as Binary, UnaryOp as Unary};
use phera_value::{
    Diagnostics, OpError, Value, add, bitwise_and, bitwise_not, bitwise_or, bitwise_xor, concat,
    div, modulo, mul, negate, pow, shift_left, shift_right, strict_equals, sub, to_bool,
    unary_plus,
};

/// Delegate coercion and warnings to the value layer for each PHP binary operator.
pub fn binary(
    operator: Binary,
    left: &Value,
    right: &Value,
    diagnostics: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    match operator {
        Binary::Coalesce => Ok(if left.is_null() {
            right.clone()
        } else {
            left.clone()
        }),
        Binary::Add => add(left, right, diagnostics),
        Binary::Subtract => sub(left, right, diagnostics),
        Binary::Multiply => mul(left, right, diagnostics),
        Binary::Divide => div(left, right, diagnostics),
        Binary::Modulo => modulo(left, right, diagnostics),
        Binary::Power => pow(left, right, diagnostics),
        Binary::Concat => concat(left, right, diagnostics),
        Binary::BitwiseAnd => bitwise_and(left, right, diagnostics),
        Binary::BitwiseOr => bitwise_or(left, right, diagnostics),
        Binary::BitwiseXor => bitwise_xor(left, right, diagnostics),
        Binary::ShiftLeft => shift_left(left, right, diagnostics),
        Binary::ShiftRight => shift_right(left, right, diagnostics),
        Binary::Identical => strict_equals(left, right, diagnostics),
        Binary::NotIdentical => Ok(Value::Bool(!to_bool(&strict_equals(
            left,
            right,
            diagnostics,
        )?))),
        Binary::Equal
        | Binary::NotEqual
        | Binary::Less
        | Binary::LessEqual
        | Binary::Greater
        | Binary::GreaterEqual
        | Binary::Spaceship => crate::object_comparison::compare(
            operator,
            left,
            right,
            diagnostics,
            &mut crate::object_comparison::compare_objects,
        ),
        Binary::LogicalXor => Ok(Value::Bool(to_bool(left) ^ to_bool(right))),
    }
}

/// Apply PHP unary conversion rules through the value layer.
pub fn unary(
    operator: Unary,
    left: &Value,
    diagnostics: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    match operator {
        Unary::Plus => unary_plus(left, diagnostics),
        Unary::Negate => negate(left, diagnostics),
        Unary::Not => Ok(Value::Bool(!to_bool(left))),
        Unary::BitwiseNot => bitwise_not(left, diagnostics),
    }
}
