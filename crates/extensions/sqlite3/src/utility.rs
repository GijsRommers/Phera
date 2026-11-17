//! `SQLite3::escapeString("it's")` doubles quotes; diagnostics retain SQLite's error code.

use crate::diagnostics::session;
use phera_extension_api::{ArrayKey, Context, NativeResult, Object, PhpArray, Value, args};
use std::rc::Rc;

pub fn escape(_: &mut dyn Context, _: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let bytes = args::string(arguments, 0).map_or(&b""[..], |s| s.as_bytes());
    let mut escaped = Vec::new();
    for byte in bytes.iter().take_while(|byte| **byte != 0) {
        escaped.push(*byte);
        if *byte == b'\'' {
            escaped.push(*byte);
        }
    }
    Ok(Value::from(escaped))
}

pub fn version(_: &mut dyn Context, _: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let mut values = PhpArray::new();
    values.insert(
        ArrayKey::str("versionString"),
        Value::from(phera_sqlite::version()),
    );
    values.insert(
        ArrayKey::str("versionNumber"),
        Value::Int(i64::from(phera_sqlite::version_number())),
    );
    Ok(Value::from(values))
}

pub fn exceptions(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    args: &[Value],
) -> NativeResult {
    let session = session(this)?;
    let enabled = args::bool(args, 0).unwrap_or(false);
    if !enabled {
        context.deprecated(
            "SQLite3::enableExceptions(): Use of warnings for SQLite3 is deprecated".into(),
        );
    }
    Ok(Value::Bool(session.exceptions.replace(enabled)))
}

pub fn extended(_: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    let session = session(this)?;
    session.database()?;
    session.extended.set(args::bool(args, 0).unwrap_or(true));
    Ok(Value::Bool(true))
}

pub fn error_code(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let session = session(this)?;
    session.database()?;
    let error = session.error.borrow();
    Ok(Value::Int(i64::from(if session.extended.get() {
        error.code
    } else {
        error.code & 255
    })))
}

pub fn extended_code(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let session = session(this)?;
    session.database()?;
    Ok(Value::Int(i64::from(session.error.borrow().code)))
}

pub fn error_message(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let session = session(this)?;
    session.database()?;
    Ok(Value::from(session.error.borrow().message.clone()))
}

pub fn load(context: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let session = session(this)?;
    session.database()?;
    session.fail(
        context,
        phera_sqlite::Error {
            code: 0,
            message: "SQLite Extensions are disabled".into(),
        },
        "SQLite3::loadExtension",
        "",
    )
}
