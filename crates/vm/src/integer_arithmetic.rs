//! Integer overflow promotes to float: `PHP_INT_MAX + 1` must not overflow Rust arithmetic.

use phera_value::Value;

/// Only these integer operations promote overflow to a PHP float.
pub(crate) enum OverflowOperation {
    Add,
    Subtract,
    Multiply,
}

/// Float conversion stays on a cold branch instead of burdening ordinary integer arithmetic.
#[cold]
pub(crate) fn overflow_value(operation: OverflowOperation, left: i64, right: i64) -> Value {
    let left = left as f64;
    let right = right as f64;
    Value::Float(match operation {
        OverflowOperation::Add => left + right,
        OverflowOperation::Subtract => left - right,
        OverflowOperation::Multiply => left * right,
    })
}
