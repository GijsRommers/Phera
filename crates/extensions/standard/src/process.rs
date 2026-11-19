//! Small functions about the running process and the connection it serves:
//! `getmypid()`, `set_time_limit(30)`, `connection_aborted()`,
//! `escapeshellarg("it's")`.
//!
//! Frameworks call these on every request, mostly to ask a question whose
//! answer under Phera is fixed. Each one says here what it answers and why.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

/// What `connection_status()` returns while the client is still there.
const CONNECTION_NORMAL: i64 = 0;
/// The other two answers of `connection_status()`. Phera does not report
/// them yet; scripts compare against the names.
const CONNECTION_ABORTED: i64 = 1;
const CONNECTION_TIMEOUT: i64 = 2;

pub(crate) fn register(registry: &mut Registry) {
    for entry in [
        GETMYPID,
        GET_CFG_VAR,
        SET_TIME_LIMIT,
        CONNECTION_ABORTED_FUNCTION,
        CONNECTION_STATUS,
        IGNORE_USER_ABORT,
        ESCAPESHELLARG,
    ] {
        registry.function(entry);
    }
    registry.constant("CONNECTION_NORMAL", CONNECTION_NORMAL);
    registry.constant("CONNECTION_ABORTED", CONNECTION_ABORTED);
    registry.constant("CONNECTION_TIMEOUT", CONNECTION_TIMEOUT);
}

/// `getmypid(): int|false`, the id of the operating system process.
///
/// Every request of one `phera serve` shares a process, so this does not
/// tell requests apart. Code uses it to name lock and temporary files,
/// which `uniqid()` or `random_bytes()` do better.
const GETMYPID: FunctionEntry = FunctionEntry::sync("getmypid", &[], getmypid);

fn getmypid(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(i64::from(std::process::id())))
}

/// `get_cfg_var(string $option): string|array|false`, a value from the
/// `php.ini` file. Phera reads no `php.ini`, so there is never one: `false`,
/// which is also what PHP answers when started with `-n`.
const GET_CFG_VAR: FunctionEntry = FunctionEntry::sync(
    "get_cfg_var",
    &[Param::new("option", ParamType::String)],
    get_cfg_var,
);

fn get_cfg_var(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}

/// `set_time_limit(int $seconds): bool` restarts the request's execution clock.
const SET_TIME_LIMIT: FunctionEntry = FunctionEntry::sync(
    "set_time_limit",
    &[Param::new("seconds", ParamType::Int)],
    set_time_limit,
);

fn set_time_limit(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let seconds = args::int(arguments, 0).unwrap_or_default().to_string();
    let accepted = context.ini_set("set_time_limit", b"max_execution_time", seconds.into())?;
    Ok(Value::Bool(accepted.is_some()))
}

/// `connection_aborted(): int`, 1 when the client has gone away.
///
/// When the client goes away Phera stops the request, so a script that is
/// still running always has its client: 0.
const CONNECTION_ABORTED_FUNCTION: FunctionEntry =
    FunctionEntry::sync("connection_aborted", &[], connection_aborted);

fn connection_aborted(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(0))
}

/// `connection_status(): int`, for the same reason always
/// `CONNECTION_NORMAL`.
const CONNECTION_STATUS: FunctionEntry =
    FunctionEntry::sync("connection_status", &[], connection_status);

fn connection_status(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(CONNECTION_NORMAL))
}

/// `ignore_user_abort(?bool $enable = null): int` returns the setting as it
/// was, and changes it when given `true` or `false`.
///
/// ```php
/// ignore_user_abort();     // 0
/// ignore_user_abort(true); // 0, and now on
/// ignore_user_abort();     // 1
/// ```
///
/// The setting is remembered and reported; it does not yet change what
/// happens when a client disconnects.
const IGNORE_USER_ABORT: FunctionEntry = FunctionEntry::sync(
    "ignore_user_abort",
    &[Param::new("enable", ParamType::Bool).nullable().optional()],
    ignore_user_abort,
);

fn ignore_user_abort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let before = context
        .ini_get(b"ignore_user_abort")
        .is_some_and(|value| phera_extension_api::ini::boolean(value.as_bytes()));
    if let Some(enable) = args::bool(arguments, 0) {
        let value = if enable { "1" } else { "0" };
        context.ini_set("ignore_user_abort", b"ignore_user_abort", value.into())?;
    }
    Ok(Value::Int(i64::from(before)))
}

/// `escapeshellarg(string $arg): string` makes one shell word of a string:
/// single quotes around it, and each single quote inside written as `'\''`.
///
/// ```php
/// escapeshellarg("it's"); // 'it'\''s'
/// ```
///
/// The bytes of the string are kept as they are. PHP on Linux drops bytes
/// that are not valid in the current locale, which under the default "C"
/// locale means every non-ASCII byte; Phera has no locale and keeps them,
/// as PHP does under a UTF-8 locale.
const ESCAPESHELLARG: FunctionEntry = FunctionEntry::sync(
    "escapeshellarg",
    &[Param::new("arg", ParamType::String)],
    escapeshellarg,
);

fn escapeshellarg(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let argument = args::bytes(arguments, 0);
    if argument.contains(&0) {
        return Err(NativeError::value_error(
            "escapeshellarg(): Argument #1 ($arg) must not contain any null bytes",
        ));
    }
    let mut word = Vec::with_capacity(argument.len() + 2);
    word.push(b'\'');
    for &byte in argument {
        if byte == b'\'' {
            word.extend_from_slice(b"'\\''");
        } else {
            word.push(byte);
        }
    }
    word.push(b'\'');
    Ok(Value::from(word))
}
