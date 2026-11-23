//! Read the indexed tokens and reject writes through dimensions.
//!
//! ```php
//! $first = $element->classList[0];
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value};

use super::{CLASS, state};

pub(super) fn offset(context: &mut dyn Context, object: &Object, offset: Value) -> NativeResult {
    use phera_extension_api::value::{NumVal, parse_numeric, to_int};
    let offset = offset.deref();
    let index = match &*offset {
        Value::Int(_) | Value::Float(_) | Value::Bool(_) => {
            Some(to_int(&offset, context.diagnostics())?)
        }
        Value::InternedStr(text) => {
            let parsed = parse_numeric(text.as_bytes());
            match parsed.value.filter(|_| parsed.exact) {
                Some(NumVal::Int(index)) => Some(index),
                _ => None,
            }
        }
        Value::String(text) => {
            let parsed = parse_numeric(text.as_bytes());
            match parsed.value.filter(|_| parsed.exact) {
                Some(NumVal::Int(index)) => Some(index),
                _ => None,
            }
        }
        _ => None,
    };
    let index = index.ok_or_else(|| {
        NativeError::type_error(format!(
            "Cannot access offset of type {} on {CLASS}",
            offset.type_name(),
        ))
    })?;
    let tokens = state(object)?.tokens();
    Ok(usize::try_from(index)
        .ok()
        .and_then(|index| tokens.get(index))
        .map_or(Value::Null, |value| Value::from(value.as_str())))
}

pub(super) fn cannot_mutate(_: &mut dyn Context, _: &Object, _: Value) -> Result<(), NativeError> {
    Err(NativeError::error(format!(
        "Cannot use object of type {CLASS} as array"
    )))
}
