use phera_bytecode::{Binary, Unary};
use phera_value::*;
pub fn binary(op: Binary, a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    use Binary::*;
    match op {
        Add => add(a, b, d),
        Subtract => sub(a, b, d),
        Multiply => mul(a, b, d),
        Divide => div(a, b, d),
        Modulo => modulo(a, b, d),
        Power => pow(a, b, d),
        Concat => concat(a, b, d),
        BitwiseAnd => bitwise_and(a, b, d),
        BitwiseOr => bitwise_or(a, b, d),
        BitwiseXor => bitwise_xor(a, b, d),
        ShiftLeft => shift_left(a, b, d),
        ShiftRight => shift_right(a, b, d),
        Equal => loose_equals(a, b, d),
        Identical => strict_equals(a, b, d),
        NotEqual => Ok(Value::Bool(!to_bool(&loose_equals(a, b, d)?))),
        NotIdentical => Ok(Value::Bool(!to_bool(&strict_equals(a, b, d)?))),
        Less => less(a, b, d),
        LessEqual => less_equal(a, b, d),
        Greater => less(b, a, d),
        GreaterEqual => less_equal(b, a, d),
        Spaceship => compare_op(a, b, d),
        Xor => Ok(Value::Bool(to_bool(a) ^ to_bool(b))),
    }
}
pub fn unary(op: Unary, a: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    use Unary::*;
    match op {
        Plus => unary_plus(a, d),
        Minus => negate(a, d),
        Not => Ok(Value::Bool(!to_bool(a))),
        BitwiseNot => bitwise_not(a, d),
        Bool => Ok(Value::Bool(cast_to_bool(a, d))),
        Int => Ok(Value::Int(to_int(a, d)?)),
        Float => Ok(Value::Float(to_float(a, d)?)),
        String => Ok(Value::String(to_php_string(a, d)?)),
        Array => Ok(Value::Array(std::rc::Rc::new(to_array(a, d)))),
    }
}
