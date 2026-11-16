//! Array arithmetic shares scalar operators but recovers from unsupported entries.

use phera_extension_api::value::{add, mul};
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

const ARRAY: &[Param] = &[Param::new("array", ParamType::Array)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("array_sum", ARRAY, sum));
    registry.function(FunctionEntry::sync("array_product", ARRAY, product));
}

fn sum(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, false)
}

fn product(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    aggregate(context, arguments, true)
}

/// PHP 8.3 warns for unsupported operands. Non-numeric strings still act as
/// zero for compatibility, while arrays are skipped rather than cast to one.
fn aggregate(context: &mut dyn Context, arguments: &[Value], multiply: bool) -> NativeResult {
    let mut result = Value::Int(i64::from(multiply));
    let operation = if multiply { mul } else { add };
    if let Some(array) = args::array(arguments, 0) {
        for value in array.values() {
            match operation(&result, value, context.diagnostics()) {
                Ok(value) => result = value,
                Err(_) => {
                    let value = value.deref();
                    if matches!(&*value, Value::String(_)) {
                        result = operation(&result, &Value::Int(0), context.diagnostics())?;
                    }
                    context.warning(format!(
                        "{}(): {} is not supported on type {}",
                        if multiply {
                            "array_product"
                        } else {
                            "array_sum"
                        },
                        if multiply {
                            "Multiplication"
                        } else {
                            "Addition"
                        },
                        value.type_name()
                    ));
                }
            }
        }
    }
    Ok(result)
}
