//! GMP overloads arithmetic and bit operations, with integer quotients.

use num_bigint::BigInt;
use num_integer::Integer;
use num_traits::Zero;
use phera_extension_api::{
    Context, Diagnostics, NativeBinaryOp as Op, NativeComparison, NativeError, NativeUnaryOp,
    Object, Value,
    value::{self, OpError},
};

use crate::{
    arithmetic,
    integer::{self, Result},
};

use super::{create, value as number};

pub(super) fn compare_objects(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> std::result::Result<NativeComparison, OpError> {
    if left.class.name() == "GMP" && right.class.name() == "GMP" {
        Ok(NativeComparison::Ordered(number(left).cmp(&number(right))))
    } else {
        let given = if left.class.name() == "GMP" {
            &right.class
        } else {
            &left.class
        };
        Err(OpError::type_error(format!(
            "Number must be of type GMP|string|int, {} given",
            given.name(),
        )))
    }
}

fn operand(context: &mut dyn Context, value: &Value) -> Result<BigInt> {
    let coerced = match value {
        Value::Object(object) if object.class.name() == "GMP" => Some(number(object)),
        Value::Int(integer) => Some(BigInt::from(*integer)),
        Value::String(string) => {
            return integer::parse(string.as_bytes(), 0)
                .ok_or_else(|| NativeError::value_error("Number is not an integer string"));
        }
        Value::Bool(flag) if !context.strict_types() => Some(BigInt::from(i32::from(*flag))),
        Value::Float(float)
            if !context.strict_types()
                && *float >= i64::MIN as f64
                && *float < -(i64::MIN as f64) =>
        {
            let int = *float as i64;
            if int as f64 != *float {
                context.deprecated(format!(
                    "Implicit conversion from float {} to int loses precision",
                    value::float_repr(*float)
                ));
            }
            Some(BigInt::from(int))
        }
        _ => None,
    };
    coerced.ok_or_else(|| {
        NativeError::type_error(format!(
            "Number must be of type GMP|string|int, {} given",
            match value {
                Value::Bool(true) => "true".into(),
                Value::Bool(false) => "false".into(),
                _ => value.type_name(),
            }
        ))
    })
}

fn shift(context: &mut dyn Context, op: Op, left: &Value, right: &Value) -> Result<BigInt> {
    let label = if op == Op::Power { "Exponent" } else { "Shift" };
    let unsupported = || {
        let symbol = match op {
            Op::Power => "**",
            Op::ShiftLeft => "<<",
            _ => ">>",
        };
        NativeError::type_error(format!(
            "Unsupported operand types: {} {symbol} {}",
            left.type_name(),
            right.type_name(),
        ))
    };
    let bits = match right {
        Value::Int(integer) => *integer,
        Value::Object(object) if object.class.name() == "GMP" => integer::to_int(&number(object)),
        Value::Float(_) => value::to_int(right, context.diagnostics())?,
        Value::String(text) => {
            let numeric = value::parse_numeric(text.as_bytes());
            match numeric.value {
                Some(value::NumVal::Int(integer)) if numeric.exact => integer,
                _ => return Err(NativeError::value_error("Number is not an integer string")),
            }
        }
        _ => return Err(unsupported()),
    };
    if bits < 0 {
        return Err(NativeError::value_error(format!(
            "{label} must be greater than or equal to 0"
        )));
    }
    let value = operand(context, left).map_err(|error| match &error {
        NativeError::Throw {
            class: "TypeError", ..
        } => unsupported(),
        _ => error,
    })?;
    match op {
        Op::Power => arithmetic::power(value, bits as u64),
        Op::ShiftLeft => {
            arithmetic::check_bits(value.bits().saturating_add(bits as u64))?;
            Ok(value << bits as u64)
        }
        _ => Ok(value >> bits as u64),
    }
}

pub(super) fn binary(
    context: &mut dyn Context,
    op: Op,
    left: &Value,
    right: &Value,
) -> Result<Option<Value>> {
    let value = match op {
        Op::Power | Op::ShiftLeft | Op::ShiftRight => shift(context, op, left, right)?,
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::Divide
        | Op::Modulo
        | Op::BitwiseAnd
        | Op::BitwiseOr
        | Op::BitwiseXor => {
            let mut checked_left = None;
            if matches!(op, Op::BitwiseAnd | Op::BitwiseOr | Op::BitwiseXor) && !left.is_object() {
                let symbol = match op {
                    Op::BitwiseAnd => "&",
                    Op::BitwiseOr => "|",
                    _ => "^",
                };
                checked_left = Some(
                    value::try_get_long(left, context.diagnostics())
                        .map_err(|_| value::binop_type_error(symbol, left, right))?,
                );
            }
            let left = operand(context, left)?;
            // PHP's OR operator uses the scalar it prepared for the left side.
            let left = if op == Op::BitwiseOr {
                checked_left.map_or(left, BigInt::from)
            } else {
                left
            };
            let right = operand(context, right)?;
            match op {
                Op::Add => left + right,
                Op::Subtract => left - right,
                Op::Multiply => left * right,
                Op::Divide if !right.is_zero() => left / right,
                Op::Modulo if !right.is_zero() => left.mod_floor(&right.magnitude().clone().into()),
                Op::Divide => return Err(arithmetic::zero("Division")),
                Op::Modulo => return Err(arithmetic::zero("Modulo")),
                Op::BitwiseAnd => left & right,
                Op::BitwiseOr => left | right,
                _ => left ^ right,
            }
        }
        Op::Equal
        | Op::NotEqual
        | Op::Less
        | Op::LessEqual
        | Op::Greater
        | Op::GreaterEqual
        | Op::Spaceship => {
            use std::cmp::Ordering;
            let order = match (left, right) {
                (Value::Null, _) => Ordering::Less,
                (_, Value::Null) => Ordering::Greater,
                (Value::Bool(_), _) | (_, Value::Bool(_)) => {
                    value::to_bool(left).cmp(&value::to_bool(right))
                }
                _ => operand(context, left)?.cmp(&operand(context, right)?),
            };
            return Ok(Some(match op {
                Op::Equal => Value::Bool(order.is_eq()),
                Op::NotEqual => Value::Bool(order.is_ne()),
                Op::Less => Value::Bool(order.is_lt()),
                Op::LessEqual => Value::Bool(order.is_le()),
                Op::Greater => Value::Bool(order.is_gt()),
                Op::GreaterEqual => Value::Bool(order.is_ge()),
                _ => Value::Int(order as i64),
            }));
        }
        _ => return Ok(None),
    };
    create(context, value).map(Some)
}

pub(super) fn unary(
    context: &mut dyn Context,
    op: NativeUnaryOp,
    object: &Object,
) -> Result<Option<Value>> {
    let number = number(object);
    match op {
        NativeUnaryOp::BitwiseNot => create(context, !number).map(Some),
        NativeUnaryOp::Negate => create(context, -number).map(Some),
        NativeUnaryOp::Plus => create(context, number).map(Some),
        NativeUnaryOp::Not => Ok(None),
    }
}
