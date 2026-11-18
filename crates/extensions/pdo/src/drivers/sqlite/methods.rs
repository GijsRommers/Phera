//! `Pdo\Sqlite` exposes driver constants; SQL callbacks need a reentrant worker bridge.

use phera_extension_api::{Context, NativeResult, Object, Value};
use std::rc::Rc;

pub fn load(_: &mut dyn Context, this: Option<&Rc<Object>>, args: &[Value]) -> NativeResult {
    crate::connection::session(this)?;
    let name = phera_extension_api::args::string(args, 0)
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    if name.is_empty() {
        return Err(phera_extension_api::NativeError::value_error(
            "Pdo\\Sqlite::loadExtension(): Argument #1 ($name) must not be empty",
        ));
    }
    if name.contains('\0') {
        return Err(phera_extension_api::NativeError::value_error(
            "Pdo\\Sqlite::loadExtension(): Argument #1 ($name) must not contain any null bytes",
        ));
    }
    Err(crate::errors::exception(
        format!("Unable to load extension \"{name}\""),
        None,
        0,
    ))
}

pub fn statement_attribute(
    context: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let state = crate::statement::state(this)?;
    let state = state.borrow();
    let crate::drivers::Prepared::Sqlite(handle) = &state.prepared else {
        return crate::statement::unsupported_method(context, this, arguments);
    };
    match phera_extension_api::args::int(arguments, 0).unwrap_or(0) {
        1001 => Ok(Value::Bool(handle.description.readonly)),
        1003 => Ok(Value::Bool(handle.busy())),
        1004 => Ok(Value::Int(i64::from(handle.description.explain))),
        _ => Err(crate::errors::unsupported("This PDOStatement attribute")),
    }
}
