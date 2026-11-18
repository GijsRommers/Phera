//! Filters see the PHP string representation of a scalar or stringable object.
//!
//! ```php
//! filter_var(true, FILTER_UNSAFE_RAW); // '1'
//! filter_var(new stdClass(), FILTER_UNSAFE_RAW); // false
//! ```

use phera_extension_api::value::to_php_string;
use phera_extension_api::{Context, NativeError, PhpArray, PhpString, Value};

pub async fn value(
    context: &mut dyn Context,
    value: Value,
) -> Result<Option<PhpString>, NativeError> {
    if !value.is_object() {
        return Ok(Some(to_php_string(&value, context.diagnostics())?));
    }

    let exists = context
        .call(
            &Value::from("method_exists"),
            vec![value.clone(), Value::from("__toString")],
        )
        .await?;
    if !matches!(exists, Value::Bool(true)) {
        return Ok(None);
    }

    let mut method = PhpArray::new();
    method.push(value)?;
    method.push(Value::from("__toString"))?;
    let result = context.call(&Value::from(method), Vec::new()).await?;
    Ok(Some(to_php_string(&result, context.diagnostics())?))
}
