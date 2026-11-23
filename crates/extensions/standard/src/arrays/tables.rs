//! Arrays as tables: `array_count_values`, `array_column` and
//! `array_chunk`.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_COUNT_VALUES, ARRAY_COLUMN, ARRAY_CHUNK];

/// `array_count_values(array $array): array`, how often each value occurs.
/// Only ints and strings can be counted; others are skipped with a warning.
///
/// ```php
/// array_count_values(["a", "b", "a", 1]); // ["a" => 2, "b" => 1, 1 => 1]
/// ```
const ARRAY_COUNT_VALUES: FunctionEntry = FunctionEntry::sync(
    "array_count_values",
    &[Param::new("array", ParamType::Array)],
    array_count_values,
);

fn array_count_values(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for value in required_array(arguments, 0).values() {
        if !matches!(
            &*value.deref(),
            Value::Int(_) | Value::InternedStr(_) | Value::String(_)
        ) {
            context.warning(
                "array_count_values(): Can only count string and integer values, entry skipped"
                    .into(),
            );
            continue;
        }
        let key = ArrayKey::from_value(value)?;
        let count = match result.get(&key) {
            Some(Value::Int(count)) => *count,
            _ => 0,
        };
        result.insert(key, Value::Int(count + 1));
    }
    Ok(Value::from(result))
}

/// `array_column(array $array, int|string|null $column_key, int|string|null
/// $index_key = null): array`, one column of a list of rows (arrays or
/// objects).
///
/// Rows without the column are skipped. With `$index_key`, the result is
/// keyed by that column; a row without it gets the next integer key. A null
/// column key takes whole rows.
///
/// ```php
/// $rows = [["id" => 3, "name" => "a"], ["id" => 5, "name" => "b"]];
/// array_column($rows, "name");       // ["a", "b"]
/// array_column($rows, "name", "id"); // [3 => "a", 5 => "b"]
/// array_column($rows, null, "id");   // [3 => [...], 5 => [...]]
/// ```
const ARRAY_COLUMN: FunctionEntry = FunctionEntry::sync(
    "array_column",
    &[
        Param::new("array", ParamType::Array),
        Param::new("column_key", ParamType::IntOrString).nullable(),
        Param::new("index_key", ParamType::IntOrString)
            .nullable()
            .optional(),
    ],
    array_column,
);

fn array_column(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let key_argument = |index: usize| -> Result<Option<ArrayKey>, NativeError> {
        match args::value(arguments, index) {
            Value::Null => Ok(None),
            key => Ok(Some(ArrayKey::from_value(key)?)),
        }
    };
    let column = key_argument(1)?;
    let index = key_argument(2)?;
    let mut result = PhpArray::new();
    for row in required_array(arguments, 0).values() {
        let row = row.deref();
        let cell = |key: &ArrayKey| match &*row {
            Value::Array(array) => array.get(key).cloned(),
            Value::Object(object) => object.get(key),
            _ => None,
        };
        let value = match &column {
            None => row.clone(),
            Some(key) => match cell(key) {
                Some(value) => value.deref().clone(),
                None => continue,
            },
        };
        match index.as_ref().and_then(cell) {
            Some(key) => {
                let key = ArrayKey::from_value_diag(&key, context.diagnostics())?;
                result.insert(key, value);
            }
            None => {
                result.push(value)?;
            }
        }
    }
    Ok(Value::from(result))
}

/// `array_chunk(array $array, int $length, bool $preserve_keys = false):
/// array`, the entries in lists of `$length`; the last may be shorter.
///
/// ```php
/// array_chunk([1, 2, 3], 2);                 // [[1, 2], [3]]
/// array_chunk(["a" => 1, "b" => 2], 1, true); // [["a" => 1], ["b" => 2]]
/// ```
const ARRAY_CHUNK: FunctionEntry = FunctionEntry::sync(
    "array_chunk",
    &[
        Param::new("array", ParamType::Array),
        Param::new("length", ParamType::Int),
        Param::new("preserve_keys", ParamType::Bool).optional(),
    ],
    array_chunk,
);

fn array_chunk(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 1).unwrap_or_default();
    if length <= 0 {
        return Err(NativeError::value_error(
            "array_chunk(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let preserve_keys = args::bool(arguments, 2).unwrap_or(false);
    let mut chunks = PhpArray::new();
    let mut chunk = PhpArray::new();
    for (key, value) in required_array(arguments, 0).iter() {
        if preserve_keys {
            chunk.insert(key, value.clone());
        } else {
            chunk.push(value.clone())?;
        }
        if chunk.len() as i64 == length {
            chunks.push(Value::from(std::mem::take(&mut chunk)))?;
        }
    }
    if !chunk.is_empty() {
        chunks.push(Value::from(chunk))?;
    }
    Ok(Value::from(chunks))
}
