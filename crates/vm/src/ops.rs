//! PHP operators dispatched without heap-owned instructions.
use phera_bytecode::{BinaryOp as Binary, UnaryOp as Unary};
use phera_value::{
    Diagnostics, OpError, Value, add, bitwise_and, bitwise_not, bitwise_or, bitwise_xor,
    compare_op, concat, div, less, less_equal, loose_equals, modulo, mul, negate, pow, shift_left,
    shift_right, strict_equals, sub, to_bool, unary_plus,
};
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
        Binary::Equal => loose_equals(left, right, diagnostics),
        Binary::Identical => strict_equals(left, right, diagnostics),
        Binary::NotEqual => Ok(Value::Bool(!to_bool(&loose_equals(
            left,
            right,
            diagnostics,
        )?))),
        Binary::NotIdentical => Ok(Value::Bool(!to_bool(&strict_equals(
            left,
            right,
            diagnostics,
        )?))),
        Binary::Less => less(left, right, diagnostics),
        Binary::LessEqual => less_equal(left, right, diagnostics),
        Binary::Greater => less(right, left, diagnostics),
        Binary::GreaterEqual => less_equal(right, left, diagnostics),
        Binary::Spaceship => compare_op(left, right, diagnostics),
        Binary::LogicalXor => Ok(Value::Bool(to_bool(left) ^ to_bool(right))),
    }
}
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
