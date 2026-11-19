//! Connection properties are computed from the latest MySQL reply, such as `$db->errno`.

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, NativeResult, Object, PhpArray, Value,
};

use super::initialize;
use crate::errors;

pub const CLIENT_INFO: &str = "mysqlnd 8.5.13";
pub const CLIENT_VERSION: i64 = 80513;

pub static ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

const NAMES: &[&str] = &[
    "affected_rows",
    "insert_id",
    "errno",
    "error",
    "error_list",
    "sqlstate",
    "field_count",
    "info",
    "server_info",
    "server_version",
    "client_info",
    "client_version",
    "host_info",
    "protocol_version",
    "warning_count",
    "connect_errno",
    "connect_error",
    "thread_id",
];

pub fn uint(number: u64) -> Value {
    i64::try_from(number).map_or_else(|_| Value::from(number.to_string()), Value::Int)
}

pub fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    how: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if !NAMES.contains(&name) {
        return Ok(None);
    }
    match how {
        NativePropertyRead::Exists => Ok(Some(Value::Bool(true))),
        _ => get(context, object, name).map(Some),
    }
}

pub fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if NAMES.contains(&name) {
        return Err(NativeError::error(format!(
            "Cannot write read-only property mysqli::${name}"
        )));
    }
    Ok(false)
}

pub fn view(
    context: &mut dyn Context,
    object: &Object,
    _: phera_extension_api::PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut properties = object.to_array();
    for &name in NAMES {
        if let Ok(value) = get(context, object, name) {
            properties.insert(ArrayKey::str(name), value);
        }
    }
    Ok(properties)
}

pub fn get(context: &mut dyn Context, object: &Object, name: &str) -> NativeResult {
    match name {
        "connect_errno" => return errors::connect_errno(context, &[]),
        "connect_error" => return errors::connect_error(context, &[]),
        "client_info" => return Ok(Value::from(CLIENT_INFO)),
        "client_version" => return Ok(Value::Int(CLIENT_VERSION)),
        _ => {}
    }
    let state = initialize(object);
    let state = state.borrow();
    state.ensure_not_closed()?;
    let error = state.error.as_ref();
    if !state.connected() && !matches!(name, "errno" | "error") {
        return Err(NativeError::error("Property access is not allowed yet"));
    }
    Ok(match name {
        "affected_rows" if error.is_some() => Value::Int(-1),
        "affected_rows" => uint(state.reply.affected),
        "insert_id" => uint(state.reply.insert_id),
        "errno" => errors::errno(error),
        "error" => errors::message(error),
        "error_list" => return errors::error_list(error),
        "sqlstate" => errors::sqlstate(error),
        "field_count" => Value::Int(state.field_count as i64),
        "info" if !state.reply.info.is_empty() => Value::from(state.reply.info.clone()),
        "server_info" => Value::from(state.version.clone()),
        "server_version" => Value::Int(version_id(&state.version)),
        "host_info" => Value::from(state.host_info.clone()),
        "protocol_version" => Value::Int(10),
        "warning_count" => Value::Int(state.reply.warnings.into()),
        "thread_id" => Value::Int(state.thread_id.into()),
        _ => Value::Null,
    })
}

pub fn version_id(version: &str) -> i64 {
    version.split('.').take(3).fold(0, |value, part| {
        let digits = part.trim_end_matches(|c: char| !c.is_ascii_digit());
        let number = digits
            .split(|c: char| !c.is_ascii_digit())
            .next()
            .and_then(|part| part.parse::<i64>().ok())
            .unwrap_or(0);
        value * 100 + number.min(99)
    })
}
