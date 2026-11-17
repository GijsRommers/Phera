//! Filtering a value first checks its shape, then converts scalar leaves to strings.
//!
//! ```php
//! filter_var(['1', ['2']], FILTER_VALIDATE_INT, FILTER_REQUIRE_ARRAY); // [1, [2]]
//! filter_var('no', FILTER_VALIDATE_BOOLEAN, ['options' => ['default' => 42]]); // 42
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value};

use crate::catalog::FILTERS;
use crate::constants::*;
use crate::options::{Failure, Options};
use crate::{domain, email, exceptions, html, ip, numbers, regexp, sanitize, stringify, tree, url};

pub async fn value(context: &mut dyn Context, value: Value, options: &Options) -> NativeResult {
    if let Value::Array(array) = value {
        if options.require_scalar {
            return if options.failure == Failure::Throw {
                Err(exceptions::failed("not a scalar value (got an array)"))
            } else {
                Ok(options.failure.value())
            };
        }
        return tree::apply(context, array, options).await;
    }
    if options.require_array {
        return if options.failure == Failure::Throw {
            Err(exceptions::got_scalar(&value))
        } else {
            Ok(options.failure.value())
        };
    }

    let filtered = leaf(context, value, options).await?;
    if options.force_array {
        let mut array = PhpArray::new();
        array.push(filtered)?;
        Ok(Value::from(array))
    } else {
        Ok(filtered)
    }
}

pub async fn leaf(context: &mut dyn Context, value: Value, options: &Options) -> NativeResult {
    let object_name = match (&value, options.failure) {
        (Value::Object(object), Failure::Throw) => Some(object.class.name().to_owned()),
        _ => None,
    };
    let Some(string) = stringify::value(context, value).await? else {
        if let Some(name) = object_name {
            return Err(exceptions::failed(&format!(
                "object of type {name} has no __toString() method",
            )));
        }
        return Ok(or_default(options.failure.value(), options));
    };
    let bytes = string.as_bytes();
    let flags = options.flags;
    let result = match options.id {
        VALIDATE_INT => numbers::integer(context, bytes, options)?.map(Value::Int),
        VALIDATE_BOOL => numbers::boolean(bytes).map(Value::Bool),
        VALIDATE_FLOAT => numbers::float(context, bytes, options)?.map(Value::Float),
        VALIDATE_DOMAIN => {
            domain::valid(bytes, flags.has(HOSTNAME)).then(|| Value::String(string.clone()))
        }
        VALIDATE_EMAIL => email::valid(bytes, flags).then(|| Value::String(string.clone())),
        VALIDATE_IP => ip::valid(bytes, flags).then(|| Value::String(string.clone())),
        VALIDATE_MAC => domain::mac(bytes, options)?.then(|| Value::String(string.clone())),
        VALIDATE_URL => url::valid(bytes, options)?.then(|| Value::String(string.clone())),
        VALIDATE_REGEXP => {
            regexp::valid(context, &string, options)?.then(|| Value::String(string.clone()))
        }
        SANITIZE_ENCODED => Some(Value::from(sanitize::encoded(bytes, flags))),
        SANITIZE_SPECIAL_CHARS => Some(Value::from(sanitize::special(bytes, flags))),
        SANITIZE_FULL_SPECIAL_CHARS => Some(Value::from(html::full(bytes, flags))),
        SANITIZE_EMAIL => Some(Value::from(sanitize::email(bytes))),
        SANITIZE_URL => Some(Value::from(sanitize::url(bytes))),
        SANITIZE_NUMBER_INT => Some(Value::from(sanitize::number_int(bytes))),
        SANITIZE_NUMBER_FLOAT => Some(Value::from(sanitize::number_float(bytes, flags))),
        SANITIZE_ADD_SLASHES => Some(Value::from(sanitize::slashes(bytes))),
        CALLBACK => Some(callback(context, Value::String(string.clone()), options).await?),
        // filter_list() and filter_id() retain PHP's catalog entries for this legacy ID.
        513 => {
            return Err(NativeError::error(
                "FILTER_SANITIZE_STRING is not supported by Phera: deprecated since PHP 8.1",
            ));
        }
        _ => Some(sanitize::raw(&string, flags)),
    };
    if result.is_none() && options.failure == Failure::Throw {
        let name = FILTERS
            .iter()
            .find_map(|&(name, id)| (id == options.id).then_some(name))
            .unwrap_or("unsafe_raw");
        return Err(exceptions::rejected(name, bytes));
    }
    Ok(or_default(
        result.unwrap_or_else(|| options.failure.value()),
        options,
    ))
}

fn or_default(value: Value, options: &Options) -> Value {
    if matches!(
        (&value, options.failure),
        (Value::Bool(false), Failure::False | Failure::Throw) | (Value::Null, Failure::Null)
    ) && let Some(default) = options.get("default")
    {
        return default.deref().clone();
    }
    value
}

async fn callback(context: &mut dyn Context, input: Value, options: &Options) -> NativeResult {
    if let Some(callback) = &options.data
        && matches!(
            context
                .call(&Value::from("is_callable"), vec![callback.clone()])
                .await?,
            Value::Bool(true)
        )
    {
        return context.call(callback, vec![input]).await;
    }
    Err(NativeError::type_error(format!(
        "{}(): Option must be a valid callback",
        options.call.name()
    )))
}
