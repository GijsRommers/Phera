//! `$statement->param_count` reports the server's prepared parameter count.

use phera_extension_api::{
    Context, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead,
    NativeResult, Object, Value,
};

use super::handle;
use crate::{connection::properties::uint, errors};

const NAMES: &[&str] = &[
    "affected_rows",
    "insert_id",
    "num_rows",
    "param_count",
    "field_count",
    "errno",
    "error",
    "error_list",
    "sqlstate",
    "id",
];

pub static ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    how: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let Some(index) = NAMES.iter().position(|n| *n == name) else {
        return Ok(None);
    };
    if matches!(how, NativePropertyRead::Exists) {
        return Ok(Some(Value::Bool(true)));
    }
    value(object, index).map(Some)
}

fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if NAMES.contains(&name) {
        return Err(NativeError::error(format!(
            "Cannot write read-only property mysqli_stmt::${name}"
        )));
    }
    Ok(false)
}

pub fn value(object: &Object, index: usize) -> NativeResult {
    let state = handle(object)?;
    let state = state.borrow();
    let error = state.error.as_ref();
    let statement = state.statement();
    Ok(match index {
        0 if error.is_some() => Value::Int(-1),
        0 => uint(state.reply.affected),
        1 => uint(state.reply.insert_id),
        2 if state.stored => uint(state.rows.as_ref().map_or(0, |rows| rows.values.len()) as u64),
        2 => Value::Int(0),
        3 => Value::Int(statement.as_ref().map_or(0, |s| i64::from(s.num_params()))),
        4 => Value::Int(statement.as_ref().map_or(0, |s| s.columns().len() as i64)),
        5 => errors::errno(error),
        6 => errors::message(error),
        7 => return errors::error_list(error),
        8 => errors::sqlstate(error),
        _ => Value::Int(statement.as_ref().map_or(0, |s| i64::from(s.id()))),
    })
}
