//! Substring replacement and simultaneous translation, including array forms.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args, prepare_arguments,
    value::{to_int, to_php_string},
};

use super::{buffer, bytes, reserve};

const REPLACE_PARAMS: &[Param] = &[
    Param::new("string", ParamType::ArrayOrString),
    Param::new("replace", ParamType::ArrayOrString),
    Param::new("offset", ParamType::Mixed),
    Param::new("length", ParamType::Mixed).optional(),
];

const REPLACE_SCALAR_PARAMS: &[Param] = &[
    Param::new("string", ParamType::Mixed),
    Param::new("replace", ParamType::Mixed),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
];

const TRANSLATE_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("from", ParamType::Mixed),
    Param::new("to", ParamType::Mixed).optional(),
];

const TRANSLATE_ARRAY_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("from", ParamType::Array),
];

const TRANSLATE_STRING_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("from", ParamType::String),
    Param::new("to", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "substr_replace",
        REPLACE_PARAMS,
        replace,
    ));
    registry.function(FunctionEntry::sync("strtr", TRANSLATE_PARAMS, translate));
}

/// Array offsets and lengths advance by insertion order, independently of keys.
fn replace(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut converted = arguments.to_vec();
    for index in [2, 3] {
        if matches!(converted.get(index), Some(Value::Array(_)))
            && let Some(value) = converted.get_mut(index)
        {
            *value = Value::Int(0);
        }
    }
    if !context.strict_types() && matches!(converted.get(2), Some(Value::Null)) {
        context.deprecated("substr_replace(): Passing null to parameter #3 ($offset) of type array|int is deprecated".to_string());
        if let Some(offset) = converted.get_mut(2) {
            *offset = Value::Int(0);
        }
    }
    let entry = FunctionEntry::sync("substr_replace", REPLACE_SCALAR_PARAMS, replace);
    if let Err(error) = prepare_arguments(&entry, &mut converted, context) {
        return Err(match error {
            NativeError::Throw {
                class,
                message,
                code,
            } => NativeError::Throw {
                class,
                code,
                message: message
                    .replace("of type ?int", "of type array|int|null")
                    .replace("of type int,", "of type array|int,"),
            },
            other => other,
        });
    }
    if args::array(arguments, 0).is_none() {
        for (index, name) in [(2, "offset"), (3, "length")] {
            if args::array(arguments, index).is_some() {
                return Err(NativeError::type_error(format!(
                    "substr_replace(): Argument #{} (${name}) cannot be an array when working on a single string",
                    index + 1
                )));
            }
        }
    }
    if let Some(array) = args::array(arguments, 0) {
        let mut result = PhpArray::new();
        for (position, (key, value)) in array.iter().enumerate() {
            result.insert(
                key,
                replace_one(context, value, position, arguments, &converted)?,
            );
        }
        Ok(Value::from(result))
    } else {
        replace_one(context, args::value(arguments, 0), 0, arguments, &converted)
    }
}

/// Translation chooses the longest match and never rescans emitted replacements.
fn translate(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut converted = arguments.to_vec();
    let params = if arguments.len() == 2 {
        TRANSLATE_ARRAY_PARAMS
    } else {
        TRANSLATE_STRING_PARAMS
    };
    prepare_arguments(
        &FunctionEntry::sync("strtr", params, translate),
        &mut converted,
        context,
    )?;
    let string = args::bytes(&converted, 0);
    if string.is_empty() {
        return Ok(bytes(Vec::new()));
    }
    if let Some(array) = args::array(&converted, 1) {
        translate_array(context, string, array)
    } else {
        let mut table: [u8; 256] = std::array::from_fn(|index| index as u8);
        for (&from, &to) in args::bytes(&converted, 1)
            .iter()
            .zip(args::bytes(&converted, 2))
        {
            table[usize::from(from)] = to;
        }
        Ok(bytes(
            string
                .iter()
                .map(|byte| table[usize::from(*byte)])
                .collect::<Vec<_>>(),
        ))
    }
}

/// Missing array entries use PHP's null/zero defaults independently for each position.
fn replace_one(
    context: &mut dyn Context,
    value: &Value,
    position: usize,
    arguments: &[Value],
    converted: &[Value],
) -> NativeResult {
    let select = |index, position| -> Option<&Value> {
        if let Some(array) = args::array(arguments, index) {
            array.values().nth(position)
        } else {
            converted.get(index)
        }
    };

    let string = to_php_string(value, context.diagnostics())?;
    let offset = to_int(
        select(2, position).unwrap_or(&Value::Int(0)),
        context.diagnostics(),
    )?;
    let start = if offset < 0 {
        string.len().saturating_sub(offset.unsigned_abs() as usize)
    } else {
        string.len().min(offset as usize)
    };
    let remaining = string.len() - start;
    let length = match select(3, position) {
        None | Some(Value::Null) => string.len() as i64,
        Some(value) => to_int(value, context.diagnostics())?,
    };
    let length = if length < 0 {
        remaining.saturating_sub(length.unsigned_abs() as usize)
    } else {
        remaining.min(length as usize)
    };
    let replacement = to_php_string(
        select(1, position).unwrap_or(&Value::Null),
        context.diagnostics(),
    )?;
    let mut result = buffer(
        string
            .len()
            .saturating_sub(length)
            .saturating_add(replacement.len()),
    )?;
    result.extend_from_slice(string.as_bytes().get(..start).unwrap_or_default());
    result.extend_from_slice(replacement.as_bytes());
    result.extend_from_slice(string.as_bytes().get(start + length..).unwrap_or_default());
    Ok(bytes(result))
}

/// Longest keys win, and PHP eagerly coerces a single replacement even without a match.
fn translate_array(context: &mut dyn Context, string: &[u8], array: &PhpArray) -> NativeResult {
    // The one-entry fast path eagerly coerces even an unused replacement.
    let single = if array.len() == 1 {
        array
            .values()
            .next()
            .map(|value| to_php_string(value, context.diagnostics()))
            .transpose()?
            .map(Value::String)
    } else {
        None
    };
    let mut patterns = Vec::new();
    for (key, value) in array.iter() {
        let key = match key {
            ArrayKey::Int(number) => number.to_string().into_bytes(),
            ArrayKey::String(string) => string.as_bytes().to_vec(),
        };
        if key.is_empty() {
            context.warning("strtr(): Ignoring replacement of empty string".to_string());
            continue;
        }
        patterns.push((key, single.as_ref().unwrap_or(value)));
    }
    patterns.sort_by_key(|(key, _)| std::cmp::Reverse(key.len()));
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while let Some(tail) = string.get(cursor..)
        && !tail.is_empty()
    {
        if let Some((key, value)) = patterns.iter().find(|(key, _)| tail.starts_with(key)) {
            let replacement = to_php_string(value, context.diagnostics())?;
            reserve(&mut result, replacement.len())?;
            result.extend_from_slice(replacement.as_bytes());
            cursor += key.len();
        } else {
            reserve(&mut result, 1)?;
            result.extend(tail.first());
            cursor += 1;
        }
    }
    Ok(bytes(result))
}
