//! Keys operations for PHP values.

use std::rc::Rc;

use phera_extension_api::value::{loose_equals, strict_equals};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    PhpString, Registry, SyncFn, Value, args,
};

use super::key_value;

pub(crate) fn register(registry: &mut Registry) {
    const ONE: &[Param] = &[Param::new("array", ParamType::Array)];
    for (n, f) in [
        ("array_values", values as SyncFn),
        ("array_key_first", first_key),
        ("array_key_last", last_key),
        ("array_first", first),
        ("array_last", last),
        ("array_is_list", is_list),
    ] {
        registry.function(FunctionEntry::sync(n, ONE, f));
    }
    registry.function(FunctionEntry::sync(
        "array_keys",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("filter_value", ParamType::Mixed).optional(),
                Param::new("strict", ParamType::Bool).optional(),
            ]
        },
        keys,
    ));
    for (n, f) in [("in_array", contains as SyncFn), ("array_search", search)] {
        registry.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("needle", ParamType::Mixed),
                    Param::new("haystack", ParamType::Array),
                    Param::new("strict", ParamType::Bool).optional(),
                ]
            },
            f,
        ));
    }
    for n in ["array_key_exists", "key_exists"] {
        registry.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("key", ParamType::Mixed),
                    Param::new("array", ParamType::Array),
                ]
            },
            exists,
        ));
    }
}

fn values(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for value in array.values() {
            result.push(value.clone())?;
        }
    }
    Ok(Value::Array(Rc::new(result)))
}

fn first_key(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.first())
        .map_or(Value::Null, |(key, _)| key_value(key)))
}

fn last_key(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.last())
        .map_or(Value::Null, |(key, _)| key_value(key)))
}

fn first(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.first())
        .map_or(Value::Null, |(_, value)| value.clone()))
}

fn last(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.last())
        .map_or(Value::Null, |(_, value)| value.clone()))
}

fn is_list(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::array(arguments, 0).is_none_or(|arguments| arguments.is_list()),
    ))
}

fn equal(
    context: &mut dyn Context,
    arguments: &Value,
    b: &Value,
    strict: bool,
) -> Result<bool, NativeError> {
    Ok(matches!(
        if strict {
            strict_equals(arguments, b, context.diagnostics())?
        } else {
            loose_equals(arguments, b, context.diagnostics())?
        },
        Value::Bool(true)
    ))
}

fn keys(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            if !args::given(arguments, 1)
                || equal(
                    context,
                    value,
                    arguments.get(1).unwrap_or(&Value::Null),
                    args::bool(arguments, 2).unwrap_or(false),
                )?
            {
                result.push(key_value(key))?;
            }
        }
    }
    Ok(Value::Array(Rc::new(result)))
}

fn find(context: &mut dyn Context, arguments: &[Value]) -> Result<Option<Value>, NativeError> {
    if let Some(array) = args::array(arguments, 1) {
        for (key, value) in array.iter() {
            if equal(
                context,
                arguments.first().unwrap_or(&Value::Null),
                value,
                args::bool(arguments, 2).unwrap_or(false),
            )? {
                return Ok(Some(key_value(key)));
            }
        }
    }
    Ok(None)
}

fn search(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(find(context, arguments)?.unwrap_or(Value::Bool(false)))
}

fn contains(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(find(context, arguments)?.is_some()))
}

fn exists(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = arguments.first().unwrap_or(&Value::Null);
    let key = if value.is_null() {
        context.deprecated("Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead".into());
        ArrayKey::String(PhpString::new())
    } else {
        ArrayKey::from_value_diag(value, context.diagnostics())?
    };
    Ok(Value::Bool(
        args::array(arguments, 1).is_some_and(|array| array.contains_key(&key)),
    ))
}
