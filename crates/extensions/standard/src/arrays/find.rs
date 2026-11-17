//! Short-circuit predicates: `array_find([2, 3], fn($v, $k) => $v > 2)` is 3.

use phera_extension_api::value::cast_to_bool;
use phera_extension_api::{
    AsyncFn, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value,
    args,
};

pub(crate) fn register(registry: &mut Registry) {
    for (name, handler) in [
        ("array_find", find as AsyncFn),
        ("array_find_key", find_key),
        ("array_any", any),
        ("array_all", all),
    ] {
        registry.function(FunctionEntry::asynchronous(
            name,
            const {
                &[
                    Param::new("array", ParamType::Array),
                    Param::new("callback", ParamType::Callable),
                ]
            },
            handler,
        ));
    }
}

fn find(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "array_find"))
}
fn find_key(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "array_find_key"))
}
fn any(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "array_any"))
}
fn all(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(run(context, arguments, "array_all"))
}

async fn run(context: &mut dyn Context, arguments: Vec<Value>, name: &str) -> NativeResult {
    let callback = args::value(&arguments, 1);
    super::callback::validate(name, 2, true, callback, false)?;
    if let Some(array) = args::array(&arguments, 0) {
        for (key, value) in array.iter() {
            let result = context
                .call(callback, vec![value.clone(), Value::from(key.clone())])
                .await?;
            let matches = cast_to_bool(&result, context.diagnostics());
            if name == "array_all" {
                if !matches {
                    return Ok(Value::Bool(false));
                }
            } else if matches {
                return Ok(match name {
                    "array_find" => value.deref().clone(),
                    "array_find_key" => Value::from(key.clone()),
                    _ => Value::Bool(true),
                });
            }
        }
    }
    Ok(match name {
        "array_all" => Value::Bool(true),
        "array_any" => Value::Bool(false),
        _ => Value::Null,
    })
}
