//! Collator sort writes back to PHP's array reference after sorting values.
//!
//! ```php
//! $names = ["z" => "Zürich", "a" => "Ålesund"];
//! (new Collator('en'))->asort($names); // retains "a" and "z"
//! ```

use std::cmp::Ordering;

use icu_collator::CollatorBorrowed;
use phera_ext_standard::zend_sort_order;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};
use phera_value::{NumVal, compare, parse_numeric, to_float, to_php_string};

use super::{Collator, Operation, failure};

pub fn key(collator: &CollatorBorrowed<'_>, value: &[u8]) -> Vec<u8> {
    let mut key = Vec::new();
    let Ok(()) = collator.write_sort_key_utf8_to(value, &mut key);
    key
}

enum KeyMode {
    Regular,
    Strings,
    Numbers,
    SortKeys,
}

pub fn array(
    context: &mut dyn Context,
    state: &mut Collator,
    arguments: &[Value],
    site: &str,
    operation: Operation,
) -> NativeResult {
    let Some(reference) = args::reference(arguments, 0) else {
        return Ok(Value::Bool(false));
    };
    let mut reference = reference
        .try_borrow_mut()
        .map_err(|_| NativeError::error(format!("{site}(): Argument is already being modified")))?;
    let Value::Array(array) = &*reference else {
        return Ok(Value::Bool(false));
    };
    let mode = match args::int(arguments, 1).unwrap_or(0) {
        _ if matches!(operation, Operation::SortKeys) => KeyMode::SortKeys,
        0 => KeyMode::Regular,
        1 => KeyMode::Strings,
        2 => KeyMode::Numbers,
        _ => KeyMode::Regular,
    };
    let mut values = Vec::with_capacity(array.len());
    for (key, value) in array.iter() {
        if let Value::String(text) = &*value.deref()
            && std::str::from_utf8(text.as_bytes()).is_err()
        {
            let detail = if matches!(operation, Operation::SortKeys) {
                "Sort with sort keys failed"
            } else {
                "Error converting hash from UTF-8 to UTF-16"
            };
            return failure(context, state, site, 10, detail);
        }
        let entry = match mode {
            KeyMode::SortKeys => {
                let bytes = match value {
                    Value::String(text) => text.as_bytes(),
                    _ => b"",
                };
                SortValue::String(self::key(&state.formatter, bytes))
            }
            KeyMode::Strings => {
                let text = to_php_string(value, context.diagnostics())?;
                if std::str::from_utf8(text.as_bytes()).is_err() {
                    return failure(
                        context,
                        state,
                        site,
                        10,
                        "Error converting string to UTF-16",
                    );
                }
                SortValue::String(self::key(&state.formatter, text.as_bytes()))
            }
            KeyMode::Numbers => SortValue::Number(to_float(value, context.diagnostics())?),
            KeyMode::Regular => SortValue::Regular,
        };
        values.push((key.clone(), value.clone(), entry));
    }
    let sorted = zend_sort_order(values.len(), |left_index, right_index| {
        let (_, left, left_key) = &values[left_index];
        let (_, right, right_key) = &values[right_index];
        let order = match (left_key, right_key) {
            (SortValue::String(left), SortValue::String(right)) => left.cmp(right),
            (SortValue::Number(left), SortValue::Number(right)) => {
                left.partial_cmp(right).unwrap_or(Ordering::Equal)
            }
            _ => regular(context, &state.formatter, left, right),
        };
        Ok(if matches!(mode, KeyMode::SortKeys) {
            order
        } else {
            order.then_with(|| left_index.cmp(&right_index))
        })
    })?;
    let preserve_keys = matches!(operation, Operation::Asort);
    let result: PhpArray = sorted
        .into_iter()
        .enumerate()
        .map(|(index, position)| {
            let (key, value, _) = &values[position];
            let key = if preserve_keys {
                key.clone()
            } else {
                ArrayKey::Int(index as i64)
            };
            (key, value.clone())
        })
        .collect();
    *reference = Value::from(result);
    Ok(Value::Bool(true))
}

enum SortValue {
    Regular,
    String(Vec<u8>),
    Number(f64),
}

fn regular(
    context: &mut dyn Context,
    collator: &CollatorBorrowed<'_>,
    left: &Value,
    right: &Value,
) -> Ordering {
    let left_value = left.deref();
    let right_value = right.deref();
    if let (Value::String(left), Value::String(right)) = (&*left_value, &*right_value) {
        let left_number = parse_numeric(left.as_bytes());
        let right_number = parse_numeric(right.as_bytes());
        let left_number = left_number.exact.then_some(left_number.value).flatten();
        let right_number = right_number.exact.then_some(right_number.value).flatten();
        if let (Some(left), Some(right)) = (left_number, right_number) {
            return match (left, right) {
                (NumVal::Int(left), NumVal::Int(right)) => left.cmp(&right),
                (left, right) => number(left)
                    .partial_cmp(&number(right))
                    .unwrap_or(Ordering::Equal),
            };
        }
        return collator.compare_utf8(left.as_bytes(), right.as_bytes());
    }
    compare(left, right, context.diagnostics())
        .unwrap_or(0)
        .cmp(&0)
}

fn number(value: NumVal) -> f64 {
    match value {
        NumVal::Int(value) => value as f64,
        NumVal::Float(value) => value,
    }
}
