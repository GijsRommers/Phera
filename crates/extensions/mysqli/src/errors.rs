//! `mysqli_report(MYSQLI_REPORT_OFF)` stores and returns errors without throwing.

use phera_db_drivers::errors::DbError;
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, Value};
use std::rc::Rc;

const MODE: &str = "mysqli.report_mode";
const CONNECT_ERRNO: &str = "mysqli.connect_errno";
const CONNECT_ERROR: &str = "mysqli.connect_error";

pub fn mode(context: &dyn Context) -> i64 {
    match context.extension_value(MODE) {
        Some(Value::Int(value)) => value,
        _ => 3,
    }
}

pub fn set_mode(context: &mut dyn Context, value: i64) {
    context.set_extension_value(MODE, Value::Int(value));
}

pub fn set_connect_error(context: &mut dyn Context, error: Option<&DbError>) {
    context.set_extension_value(CONNECT_ERRNO, errno(error));
    context.set_extension_value(
        CONNECT_ERROR,
        error.map_or(Value::Null, |error| Value::from(error.detail.clone())),
    );
}

pub fn connect_errno(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .extension_value(CONNECT_ERRNO)
        .unwrap_or(Value::Int(0)))
}

pub fn connect_error(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(context
        .extension_value(CONNECT_ERROR)
        .unwrap_or(Value::Null))
}

pub fn errno(error: Option<&DbError>) -> Value {
    Value::Int(error.and_then(|error| error.code).unwrap_or_default())
}

pub fn message(error: Option<&DbError>) -> Value {
    error.map_or_else(
        || Value::from(""),
        |error| Value::from(error.detail.clone()),
    )
}

pub fn sqlstate(error: Option<&DbError>) -> Value {
    error.map_or_else(
        || Value::from("00000"),
        |error| Value::from(error.state.clone()),
    )
}

pub fn report(
    context: &mut dyn Context,
    error: &DbError,
    call: &str,
    connect: bool,
) -> NativeResult {
    if mode(context) & 3 == 3 {
        return Err(NativeError::throw_with_code(
            "mysqli_sql_exception",
            error.detail.clone(),
            error.code.unwrap_or(0),
        )
        .with_property("\0*\0sqlstate", Value::from(error.state.clone())));
    }
    if connect || mode(context) & 1 != 0 {
        context.warning(format!(
            "{call}(): ({}{}): {}",
            error.state,
            if connect {
                format!("/{}", error.code.unwrap_or(0))
            } else {
                String::new()
            },
            error.detail
        ));
    }
    Ok(Value::Bool(false))
}

pub fn error_list(error: Option<&DbError>) -> NativeResult {
    let mut errors = PhpArray::new();
    if let Some(error) = error {
        let mut entry = PhpArray::new();
        entry.insert(ArrayKey::str("errno"), errno(Some(error)));
        entry.insert(ArrayKey::str("sqlstate"), sqlstate(Some(error)));
        entry.insert(ArrayKey::str("error"), message(Some(error)));
        errors.push(Value::from(entry))?;
    }
    Ok(Value::from(errors))
}

pub fn exception_state(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    _: &[Value],
) -> NativeResult {
    Ok(this
        .and_then(|o| o.get(&Object::protected_key("sqlstate")))
        .unwrap_or_else(|| Value::from("00000")))
}
