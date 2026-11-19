//! `session_cache_expire(30)` returns the previous cache lifetime in minutes.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::{
    id::c_string,
    settings::{integer, text, update},
    state::{active, can_change},
};
pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("session_name", NAME, |context, arguments| {
        string_setting(context, arguments, "session_name", "name", "name", true)
    }),
    FunctionEntry::sync("session_save_path", PATH, |context, arguments| {
        string_setting(
            context,
            arguments,
            "session_save_path",
            "save_path",
            "save path",
            true,
        )
    }),
    FunctionEntry::sync("session_cache_limiter", VALUE, |context, arguments| {
        string_setting(
            context,
            arguments,
            "session_cache_limiter",
            "cache_limiter",
            "cache limiter",
            false,
        )
    }),
    FunctionEntry::sync("session_cache_expire", EXPIRE, cache_expire),
    FunctionEntry::sync("session_module_name", MODULE, module_name),
];

const NAME: &[Param] = &[Param::new("name", ParamType::String).nullable().optional()];
const PATH: &[Param] = &[Param::new("path", ParamType::String).nullable().optional()];
const VALUE: &[Param] = &[Param::new("value", ParamType::String).nullable().optional()];
const EXPIRE: &[Param] = &[Param::new("value", ParamType::Int).nullable().optional()];
const MODULE: &[Param] = &[Param::new("module", ParamType::String)
    .nullable()
    .optional()];

fn string_setting(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    setting: &str,
    label: &str,
    path: bool,
) -> NativeResult {
    let previous = text(context, setting);
    if let Some(next) = args::string(arguments, 0) {
        if path && next.as_bytes().contains(&0) {
            let parameter = if setting == "name" { "name" } else { "path" };
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #1 (${parameter}) must not contain any null bytes"
            )));
        }
        if !can_change(context, function, label, true) {
            return Ok(Value::Bool(false));
        }
        update(context, setting, next.clone(), function)?;
    }
    Ok(Value::String(previous))
}

fn cache_expire(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let previous = Value::Int(integer(context, "cache_expire"));
    if let Some(next) = args::int(arguments, 0) {
        if !can_change(context, "session_cache_expire", "cache expiration", true) {
            return Ok(if active(context) {
                previous
            } else {
                Value::Bool(false)
            });
        }
        update(
            context,
            "cache_expire",
            next.to_string().into(),
            "session_cache_expire",
        )?;
    }
    Ok(previous)
}

fn module_name(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let current = text(context, "save_handler");
    let previous = Value::from(if current.as_bytes() == b"user" {
        "user"
    } else {
        "files"
    });
    if let Some(next) = args::string(arguments, 0) {
        let function = "session_module_name";
        if !can_change(context, function, "save handler module", true) {
            return Ok(Value::Bool(false));
        }
        if next.as_bytes().eq_ignore_ascii_case(b"user") {
            return Err(NativeError::value_error(
                "session_module_name(): Argument #1 ($module) cannot be \"user\"",
            ));
        }
        let name = c_string(next.as_bytes());
        if !name.eq_ignore_ascii_case(b"files") {
            let name = String::from_utf8_lossy(name);
            context.warning(format!(
                "{function}(): Session handler module \"{name}\" cannot be found"
            ));
            return Ok(Value::Bool(false));
        }
        update(context, "save_handler", next.clone(), function)?;
    }
    Ok(previous)
}
