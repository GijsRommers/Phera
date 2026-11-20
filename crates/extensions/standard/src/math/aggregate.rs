//! `array_sum` and `array_product`, which use PHP's `+` and `*`.

use phera_extension_api::value::{add, mul};
use phera_extension_api::{
    Context, FunctionEntry, NativeCastKind, NativeResult, Param, ParamType, Value,
};

use crate::arguments::required_array;

#[derive(Clone, Copy)]
enum Operation {
    Sum,
    Product,
}

impl Operation {
    /// Reuse scalar operators so coercion warnings match ordinary PHP arithmetic.
    fn apply(self, context: &mut dyn Context, left: &Value, right: &Value) -> NativeResult {
        // PHP casts an object entry to a number first, so the result of summing
        // `GMP` objects is an int or a float. An object without that cast is
        // rejected by the operator below and skipped with a warning.
        let number = match &*right.deref() {
            Value::Object(object) => object.class.cast(object, NativeCastKind::Number),
            _ => None,
        };
        let right = number.as_ref().unwrap_or(right);
        Ok(match self {
            Self::Sum => add(left, right, context.diagnostics())?,
            Self::Product => mul(left, right, context.diagnostics())?,
        })
    }
}

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_SUM, ARRAY_PRODUCT];

/// `array_sum(array $array): int|float`, the values added with `+`.
///
/// ```php
/// array_sum([1, 2.5, "3"]); // 6.5
/// array_sum([1, [2]]);      // 1, with a warning: arrays are skipped
/// ```
const ARRAY_SUM: FunctionEntry = FunctionEntry::sync(
    "array_sum",
    &[Param::new("array", ParamType::Array)],
    array_sum,
);

fn array_sum(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, Operation::Sum)
}

/// `array_product(array $array): int|float`, the values multiplied.
///
/// ```php
/// array_product([2, "3", 4.0]); // 24.0
/// array_product([]);            // 1
/// ```
const ARRAY_PRODUCT: FunctionEntry = FunctionEntry::sync(
    "array_product",
    &[Param::new("array", ParamType::Array)],
    array_product,
);

fn array_product(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, Operation::Product)
}

/// Since PHP 8.3 an entry the operator rejects is skipped with a warning,
/// except that a string still counts as 0 (and arrays are no longer cast
/// to 1).
fn aggregate(context: &mut dyn Context, arguments: &[Value], operation: Operation) -> NativeResult {
    let (initial, function, description) = match operation {
        Operation::Sum => (0, "array_sum", "Addition"),
        Operation::Product => (1, "array_product", "Multiplication"),
    };
    let mut result = Value::Int(initial);
    for value in required_array(arguments, 0).values() {
        match operation.apply(context, &result, value) {
            Ok(value) => result = value,
            Err(_) => {
                let value = value.deref();
                if value.is_string() {
                    result = operation.apply(context, &result, &Value::Int(0))?;
                }
                context.warning(format!(
                    "{function}(): {description} is not supported on type {}",
                    value.type_name()
                ));
            }
        }
    }
    Ok(result)
}
