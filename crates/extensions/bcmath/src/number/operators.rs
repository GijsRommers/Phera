//! Overloaded arithmetic accepts decimals, strings, and weak integer operands.

use std::cmp::Ordering;

use num_traits::Zero;

use phera_extension_api::{
    Context, Diagnostics, NativeBinaryOp, NativeComparison, NativeError, Object, Value,
    value::{self as php_value, OpError},
};

use crate::arithmetic::{self, Operation, Result};
use crate::decimal::Decimal;

use super::{NAME, create, value};

pub(super) fn compare_objects(
    left: &Object,
    right: &Object,
    _: &mut dyn Diagnostics,
) -> std::result::Result<NativeComparison, OpError> {
    match (value(left), value(right)) {
        (Some(left), Some(right)) => Ok(NativeComparison::Ordered(order(&left, &right))),
        _ => Ok(NativeComparison::Uncomparable),
    }
}

fn order(left: &Decimal, right: &Decimal) -> Ordering {
    left.compare(right, left.scale.max(right.scale))
}

fn supported(value: &Value) -> bool {
    match value {
        Value::Int(_) | Value::String(_) | Value::Bool(_) => true,
        Value::Object(object) => object.class.name() == NAME,
        Value::Float(float) => (*float >= i64::MIN as f64) && (*float < -(i64::MIN as f64)),
        _ => false,
    }
}

fn numeric(context: &mut dyn Context, raw: &Value) -> Option<Decimal> {
    match raw {
        Value::Int(number) => Some(Decimal::integer(*number)),
        Value::String(number) => Decimal::parse(number.as_bytes()),
        Value::Object(object) => value(object),
        Value::Bool(flag) => Some(Decimal::integer(i32::from(*flag))),
        Value::Float(number) => {
            let integer = *number as i64;
            if integer as f64 != *number {
                context.deprecated(format!(
                    "Implicit conversion from float {} to int loses precision",
                    php_value::float_repr(*number)
                ));
            }
            Some(Decimal::integer(integer))
        }
        _ => None,
    }
}

pub(super) fn binary(
    context: &mut dyn Context,
    operator: NativeBinaryOp,
    left: &Value,
    right: &Value,
) -> Result<Option<Value>> {
    use NativeBinaryOp as Op;

    let arithmetic = match operator {
        Op::Add => Some(Operation::Add),
        Op::Subtract => Some(Operation::Sub),
        Op::Multiply => Some(Operation::Mul),
        Op::Divide => Some(Operation::Div),
        Op::Modulo => Some(Operation::Mod),
        Op::Power => Some(Operation::Pow),
        Op::Equal
        | Op::NotEqual
        | Op::Less
        | Op::LessEqual
        | Op::Greater
        | Op::GreaterEqual
        | Op::Spaceship => None,
        _ => return Ok(None),
    };
    let operands = if supported(left) && supported(right) {
        (numeric(context, left), numeric(context, right))
    } else if arithmetic.is_some() {
        return Ok(None);
    } else {
        (None, None)
    };
    if let Some(arithmetic) = arithmetic {
        let invalid = |side| {
            NativeError::value_error(format!(
                "{side} string operand cannot be converted to {NAME}"
            ))
        };
        let left = operands.0.ok_or_else(|| invalid("Left"))?;
        let right = operands.1.ok_or_else(|| invalid("Right"))?;
        let result = arithmetic::calculate(arithmetic, &left, &right, None, None)?;
        return create(context, result).map(Some);
    }
    let comparison = match (left, right) {
        (Value::Null, _) => Some(Ordering::Less),
        (_, Value::Null) => Some(Ordering::Greater),
        (Value::Bool(flag), Value::Object(object)) => {
            value(object).map(|number| flag.cmp(&!number.coefficient.is_zero()))
        }
        (Value::Object(object), Value::Bool(flag)) => {
            value(object).map(|number| (!number.coefficient.is_zero()).cmp(flag))
        }
        _ => operands
            .0
            .zip(operands.1)
            .map(|(left, right)| order(&left, &right)),
    };
    Ok(Some(match operator {
        Op::Equal => Value::Bool(comparison == Some(Ordering::Equal)),
        Op::NotEqual => Value::Bool(comparison != Some(Ordering::Equal)),
        Op::Less => Value::Bool(comparison == Some(Ordering::Less)),
        Op::LessEqual => Value::Bool(matches!(comparison, Some(Ordering::Less | Ordering::Equal))),
        Op::Greater => Value::Bool(comparison == Some(Ordering::Greater)),
        Op::GreaterEqual => Value::Bool(matches!(
            comparison,
            Some(Ordering::Greater | Ordering::Equal)
        )),
        Op::Spaceship => Value::Int(comparison.map_or(1, |order| order as i64)),
        _ => return Ok(None),
    }))
}
