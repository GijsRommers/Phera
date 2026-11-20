//! `$result->num_rows` and other properties reflect the current row cursor.

use phera_extension_api::{
    Context, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead, Object,
    Value,
};

use super::{closed, handle};

pub static ACCESS: NativePropertyAccess = NativePropertyAccess::new(read, mutate);

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if !matches!(
        name,
        "current_field" | "field_count" | "lengths" | "num_rows" | "type"
    ) {
        return Ok(None);
    }
    let handle = handle(object)?;
    let state = handle.borrow();
    let results = state.as_ref().ok_or_else(closed)?;
    Ok(Some(match name {
        "current_field" => Value::Int(results.field as i64),
        "field_count" => Value::Int(results.rows.columns.len() as i64),
        "lengths" => results.lengths(context)?,
        "num_rows" => results.num_rows(),
        _ => Value::Int(i64::from(results.unbuffered)),
    }))
}

fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if matches!(
        name,
        "current_field" | "field_count" | "lengths" | "num_rows" | "type"
    ) {
        return Err(NativeError::error(format!(
            "Cannot write read-only property mysqli_result::${name}"
        )));
    }
    Ok(false)
}
