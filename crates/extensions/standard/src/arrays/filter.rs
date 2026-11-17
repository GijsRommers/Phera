//! Filtering preserves keys: `array_filter([0, 2, null])` returns `[1 => 2]`.

use phera_extension_api::value::cast_to_bool;
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use crate::constants::{ARRAY_FILTER_USE_BOTH, ARRAY_FILTER_USE_KEY};

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous(
        "array_filter",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("callback", ParamType::Callable)
                    .nullable()
                    .optional(),
                Param::new("mode", ParamType::Int).optional(),
            ]
        },
        filter,
    ));
    registry.function(FunctionEntry::asynchronous(
        "array_reduce",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("callback", ParamType::Callable),
                Param::new("initial", ParamType::Mixed).optional(),
            ]
        },
        reduce,
    ));
}

fn filter(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let callback = args::value(&arguments, 1);
        super::callback::validate("array_filter", 2, true, callback, true)?;
        let mode = args::int(&arguments, 2).unwrap_or_default();
        let mut result = PhpArray::new();
        if let Some(array) = args::array(&arguments, 0) {
            for (key, value) in array.iter() {
                let predicate = if callback.is_null() {
                    value.deref().clone()
                } else {
                    let parameters = match mode {
                        ARRAY_FILTER_USE_BOTH => vec![value.clone(), super::key_value(&key)],
                        ARRAY_FILTER_USE_KEY => vec![super::key_value(&key)],
                        _ => vec![value.clone()],
                    };
                    context.call(callback, parameters).await?
                };
                if cast_to_bool(&predicate, context.diagnostics()) {
                    result.insert(key, value.clone());
                }
            }
        }
        Ok(Value::from(result))
    })
}

fn reduce(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let callback = args::value(&arguments, 1);
        super::callback::validate("array_reduce", 2, true, callback, false)?;
        let mut accumulator = args::value(&arguments, 2).clone();
        if let Some(array) = args::array(&arguments, 0) {
            for value in array.values() {
                accumulator = context
                    .call(callback, vec![accumulator, value.clone()])
                    .await?
                    .deref()
                    .clone();
            }
        }
        Ok(accumulator)
    })
}
