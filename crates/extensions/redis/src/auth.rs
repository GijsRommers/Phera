//! phpredis accepts passwords and one/two-entry numeric or user/pass credential arrays.
use phera_extension_api::{ArrayKey, Context, NativeError, PhpArray, Value};

pub fn credentials(
    context: &mut dyn Context,
    value: &Value,
    method: &str,
) -> Result<Option<Value>, NativeError> {
    let text = |context: &mut dyn Context, value: Option<&Value>| {
        value
            .filter(|v| !matches!(v, Value::Null | Value::Bool(false)))
            .map(|v| phera_extension_api::value::to_php_string(v, context.diagnostics()))
            .transpose()
    };
    let Value::Array(values) = value else {
        return Ok(text(context, Some(value))?.map(Value::String));
    };
    if !matches!(values.len(), 1 | 2) {
        context.warning(format!(
            "Redis::{method}(): When passing an array as auth it must have one or two elements!"
        ));
        return Ok(None);
    }
    let find = |name: &str, index: i64| {
        values
            .get(&ArrayKey::str(name))
            .or_else(|| values.get(&ArrayKey::Int(index)))
    };
    let user = if values.len() == 2 {
        text(context, find("user", 0))?
    } else {
        None
    };
    let password = text(context, find("pass", if values.len() == 2 { 1 } else { 0 }))?;
    let Some(password) = password else {
        return Ok(None);
    };
    if let Some(user) = user {
        let mut result = PhpArray::new();
        result.push(Value::String(user))?;
        result.push(Value::String(password))?;
        Ok(Some(Value::from(result)))
    } else {
        Ok(Some(Value::String(password)))
    }
}
