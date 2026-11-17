//! Keys operations for PHP values.

use std::rc::Rc;

use phera_extension_api::value::{loose_equals, strict_equals};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    PhpString, Registry, SyncFn, Value, args,
};

use super::key_value;

const ARRAY_READ_PARAMS: &[Param] = &[Param::new("array", ParamType::Array)];

const ARRAY_KEYS_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("filter_value", ParamType::Mixed).optional(),
    Param::new("strict", ParamType::Bool).optional(),
];

const ARRAY_SEARCH_PARAMS: &[Param] = &[
    Param::new("needle", ParamType::Mixed),
    Param::new("haystack", ParamType::Array),
    Param::new("strict", ParamType::Bool).optional(),
];

const ARRAY_KEY_EXISTS_PARAMS: &[Param] = &[
    Param::new("key", ParamType::Mixed),
    Param::new("array", ParamType::Array),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, function) in [
        ("array_values", array_values as SyncFn),
        ("array_key_first", array_key_first),
        ("array_key_last", array_key_last),
        ("array_first", array_first),
        ("array_last", array_last),
        ("array_is_list", array_is_list),
    ] {
        registry.function(FunctionEntry::sync(name, ARRAY_READ_PARAMS, function));
    }
    registry.function(FunctionEntry::sync(
        "array_keys",
        ARRAY_KEYS_PARAMS,
        array_keys,
    ));
    for (name, function) in [
        ("in_array", in_array as SyncFn),
        ("array_search", array_search),
    ] {
        registry.function(FunctionEntry::sync(name, ARRAY_SEARCH_PARAMS, function));
    }
    for name in ["array_key_exists", "key_exists"] {
        registry.function(FunctionEntry::sync(
            name,
            ARRAY_KEY_EXISTS_PARAMS,
            array_key_exists,
        ));
    }
}

/// Values discard all keys, including strings, and receive consecutive integer positions.
fn array_values(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let values = args::array(arguments, 0)
        .into_iter()
        .flat_map(|array| array.values().cloned());
    Ok(Value::from(super::list_array(values)?))
}

fn array_key_first(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.first())
        .map_or(Value::Null, |(key, _)| key_value(&key)))
}

fn array_key_last(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.last())
        .map_or(Value::Null, |(key, _)| key_value(&key)))
}

fn array_first(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.first())
        .map_or(Value::Null, |(_, value)| value.clone()))
}

fn array_last(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|arguments| arguments.last())
        .map_or(Value::Null, |(_, value)| value.clone()))
}

fn array_is_list(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::array(arguments, 0).is_none_or(|arguments| arguments.is_list()),
    ))
}

#[derive(Clone, Copy)]
enum Comparison {
    Loose,
    Strict,
}

impl Comparison {
    fn from_arguments(arguments: &[Value]) -> Self {
        if args::bool(arguments, 2).unwrap_or(false) {
            Self::Strict
        } else {
            Self::Loose
        }
    }

    /// Loose searches coerce values, while strict searches also compare PHP types.
    fn equal(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<bool, NativeError> {
        let result = match self {
            Self::Strict => strict_equals(left, right, context.diagnostics())?,
            Self::Loose => loose_equals(left, right, context.diagnostics())?,
        };
        Ok(matches!(result, Value::Bool(true)))
    }
}

/// An optional filter uses PHP equality and returns the original keys as list values.
fn array_keys(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let comparison = Comparison::from_arguments(arguments);
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            if !args::given(arguments, 1)
                || comparison.equal(context, value, arguments.get(1).unwrap_or(&Value::Null))?
            {
                result.push(key_value(&key))?;
            }
        }
    }
    Ok(Value::Array(Rc::new(result)))
}

/// Return the first matching key, including zero and the empty string.
fn find(context: &mut dyn Context, arguments: &[Value]) -> Result<Option<Value>, NativeError> {
    let comparison = Comparison::from_arguments(arguments);
    if let Some(array) = args::array(arguments, 1) {
        for (key, value) in array.iter() {
            if comparison.equal(context, arguments.first().unwrap_or(&Value::Null), value)? {
                return Ok(Some(key_value(&key)));
            }
        }
    }
    Ok(None)
}

fn array_search(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(find(context, arguments)?.unwrap_or(Value::Bool(false)))
}

fn in_array(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(find(context, arguments)?.is_some()))
}

/// Null keys still map to the empty string, but PHP 8.5 emits a deprecation.
fn array_key_exists(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
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
