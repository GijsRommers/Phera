//! `mkdir('virtual://logs', recursive: true)` passes the permissions and flags to PHP.

use phera_extension_api::{Context, NativeResult, Value};

use crate::user_file::{invoke, missing};
use crate::user_wrappers::{UserWrapper, locate};
use crate::wrapper::Target;

pub(crate) fn path(uri: &[u8]) -> Value {
    let path = match Target::parse(uri) {
        Target::File(path) => path,
        _ => uri,
    };
    Value::from(path.to_vec())
}

pub(crate) async fn method(
    context: &mut dyn Context,
    wrapper: UserWrapper,
    function: &str,
    method: &str,
    arguments: Vec<Value>,
    supplied: Value,
) -> NativeResult {
    if !wrapper.permitted(context, function) {
        return Ok(Value::Bool(false));
    }
    let object = wrapper.object(context, supplied).await?;
    let result = invoke(context, &object, method, arguments).await?;
    if result.is_none() {
        missing(context, &object, function, method, "");
    }
    Ok(Value::Bool(matches!(result, Some(Value::Bool(true)))))
}

pub(crate) async fn metadata(
    context: &mut dyn Context,
    uri: &[u8],
    function: &str,
    flag: i64,
    value: Value,
) -> Result<Option<Value>, phera_extension_api::NativeError> {
    let Some(wrapper) = locate(context, uri) else {
        return Ok(None);
    };
    method(
        context,
        wrapper,
        function,
        "stream_metadata",
        vec![path(uri), Value::Int(flag), value],
        Value::Null,
    )
    .await
    .map(Some)
}
