//! Counting, row projection, and chunking of ordered PHP arrays.
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};
const COUNT: &[Param] = &[Param::new("array", ParamType::Array)];
const COLUMN: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("column_key", ParamType::IntOrString).nullable(),
    Param::new("index_key", ParamType::IntOrString)
        .nullable()
        .optional(),
];
const CHUNK: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("length", ParamType::Int),
    Param::new("preserve_keys", ParamType::Bool).optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("array_count_values", COUNT, count));
    registry.function(FunctionEntry::sync("array_column", COLUMN, column));
    registry.function(FunctionEntry::sync("array_chunk", CHUNK, chunk));
}
/// Counting accepts only integer and string values, skipping all other types.
fn count(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for value in array.values() {
            if matches!(&*value.deref(), Value::Int(_) | Value::String(_)) {
                let key = ArrayKey::from_value(value)?;
                let count = match result.get(&key) {
                    Some(Value::Int(count)) => *count,
                    _ => 0,
                };
                result.insert(key, Value::Int(count.saturating_add(1)));
            } else {
                context.warning(
                    "array_count_values(): Can only count string and integer values, entry skipped"
                        .into(),
                );
            }
        }
    }
    Ok(Value::from(result))
}
/// Missing columns skip rows; missing index keys append with the next integer key.
fn column(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let column = if args::value(arguments, 1).is_null() {
        None
    } else {
        Some(ArrayKey::from_value(args::value(arguments, 1))?)
    };
    let index = if args::value(arguments, 2).is_null() {
        None
    } else {
        Some(ArrayKey::from_value(args::value(arguments, 2))?)
    };
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for row in array.values() {
            let row = row.deref();
            let value = match &column {
                None => Some(row.clone()),
                Some(key) => match &*row {
                    Value::Array(array) => array.get(key).cloned(),
                    Value::Object(object) => object
                        .properties
                        .try_borrow()
                        .ok()
                        .and_then(|properties| properties.get(key).cloned()),
                    _ => None,
                },
            };
            let Some(value) = value else {
                continue;
            };
            let key = index.as_ref().and_then(|key| match &*row {
                Value::Array(array) => array.get(key).cloned(),
                Value::Object(object) => object
                    .properties
                    .try_borrow()
                    .ok()
                    .and_then(|properties| properties.get(key).cloned()),
                _ => None,
            });
            if let Some(key) = key {
                let key = ArrayKey::from_value_diag(&key, context.diagnostics())?;
                result.insert(key, value.deref().clone());
            } else {
                result.push(value.deref().clone())?;
            }
        }
    }
    Ok(Value::from(result))
}
/// Chunk boundaries count entries, independent of numeric or string key values.
fn chunk(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 1).unwrap_or_default();
    if length <= 0 {
        return Err(NativeError::value_error(
            "array_chunk(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let preserve = args::bool(arguments, 2).unwrap_or(false);
    let mut result = PhpArray::new();
    let mut chunk = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            if preserve {
                chunk.insert(key.clone(), value.clone());
            } else {
                chunk.push(value.clone())?;
            }
            if chunk.len() as u64 == length as u64 {
                result.push(Value::from(std::mem::take(&mut chunk)))?;
            }
        }
    }
    if !chunk.is_empty() {
        result.push(Value::from(chunk))?;
    }
    Ok(Value::from(result))
}
