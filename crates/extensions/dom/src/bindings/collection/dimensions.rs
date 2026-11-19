//! Resolve index and attribute-name lookups on legacy DOM collections.
//!
//! ```php
//! echo $root->childNodes['1.0']->nodeName;
//! echo $root->attributes['href']->value;
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, value};

use super::{item_at, named_item};

pub(super) const INDEX_BOUNDS: &str = "must be between 0 and 2147483647";

pub(super) fn is_map(object: &Object) -> bool {
    object.class.is_instance_of("DOMNamedNodeMap")
        || matches!(
            object.class.name(),
            "Dom\\NamedNodeMap" | "Dom\\DtdNamedNodeMap" | "Dom\\HTMLCollection"
        )
}

pub(super) fn valid_index(index: i64) -> bool {
    (0..=i32::MAX.into()).contains(&index)
}

pub(super) fn get(context: &mut dyn Context, object: &Object, offset: Value) -> NativeResult {
    read(context, object, offset, false)
}

pub(super) fn exists(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
) -> Result<bool, NativeError> {
    Ok(!matches!(read(context, object, offset, true)?, Value::Null))
}

fn read(context: &mut dyn Context, object: &Object, offset: Value, quiet: bool) -> NativeResult {
    let offset = offset.deref();
    let index = match &*offset {
        Value::String(text) => {
            let number = value::parse_numeric(text.as_bytes());
            match number.value.filter(|_| number.exact) {
                Some(value::NumVal::Int(index)) => index,
                Some(value::NumVal::Float(index)) => index as i64,
                None if is_map(object) => {
                    return named_item(context, object, &String::from_utf8_lossy(text.as_bytes()));
                }
                None => return Ok(Value::Null),
            }
        }
        offset => value::to_int(offset, context.diagnostics())?,
    };
    if valid_index(index) {
        return item_at(context, object, index);
    }
    if !quiet && is_map(object) {
        return Err(NativeError::value_error(INDEX_BOUNDS));
    }
    Ok(Value::Null)
}

pub(super) fn set(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
    _: Value,
) -> Result<(), NativeError> {
    unset(context, object, offset)
}

pub(super) fn unset(_: &mut dyn Context, object: &Object, _: Value) -> Result<(), NativeError> {
    Err(NativeError::error(format!(
        "Cannot use object of type {} as array",
        object.class.name()
    )))
}
