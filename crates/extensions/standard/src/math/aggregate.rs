//! Array arithmetic shares scalar operators but recovers from unsupported entries.

use phera_extension_api::value::{add, mul};
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

#[derive(Clone, Copy)]
enum Operation {
    Sum,
    Product,
}

impl Operation {
    /// Reuse scalar operators so coercion warnings match ordinary PHP arithmetic.
    fn apply(self, context: &mut dyn Context, left: &Value, right: &Value) -> NativeResult {
        Ok(match self {
            Self::Sum => add(left, right, context.diagnostics())?,
            Self::Product => mul(left, right, context.diagnostics())?,
        })
    }
}

const ARRAY_PARAMS: &[Param] = &[Param::new("array", ParamType::Array)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("array_sum", ARRAY_PARAMS, array_sum));
    registry.function(FunctionEntry::sync(
        "array_product",
        ARRAY_PARAMS,
        array_product,
    ));
}

fn array_sum(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, Operation::Sum)
}

fn array_product(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, Operation::Product)
}

/// PHP 8.3 warns for unsupported operands. Non-numeric strings still act as
/// zero for compatibility, while arrays are skipped rather than cast to one.
fn aggregate(context: &mut dyn Context, arguments: &[Value], kind: Operation) -> NativeResult {
    let (initial, name, description) = match kind {
        Operation::Sum => (0, "array_sum", "Addition"),
        Operation::Product => (1, "array_product", "Multiplication"),
    };
    let mut result = Value::Int(initial);
    if let Some(array) = args::array(arguments, 0) {
        for value in array.values() {
            match kind.apply(context, &result, value) {
                Ok(value) => result = value,
                Err(_) => {
                    let value = value.deref();
                    if matches!(&*value, Value::String(_)) {
                        result = kind.apply(context, &result, &Value::Int(0))?;
                    }
                    context.warning(format!(
                        "{}(): {} is not supported on type {}",
                        name,
                        description,
                        value.type_name()
                    ));
                }
            }
        }
    }
    Ok(result)
}
