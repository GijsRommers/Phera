//! `socket_last_error($socket)` tracks that socket's last failed operation.

mod select;

use nix::{errno::Errno, libc};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::{SOCKET, classes};

#[derive(Default)]
struct LastError(i32);

pub(crate) fn register(registry: &mut Registry) {
    const OPTIONAL_SOCKET: &[Param] = &[SOCKET.nullable().optional()];
    registry.fork_state::<LastError>(phera_extension_api::fresh_state);
    registry.function(FunctionEntry::sync(
        "socket_last_error",
        OPTIONAL_SOCKET,
        last_error,
    ));
    registry.function(FunctionEntry::sync(
        "socket_clear_error",
        OPTIONAL_SOCKET,
        clear_error,
    ));
    registry.function(FunctionEntry::sync(
        "socket_strerror",
        const { &[Param::new("error_code", ParamType::Int)] },
        strerror,
    ));
    registry.function(FunctionEntry::sync("socket_atmark", &[SOCKET], atmark));
    select::register(registry);
}

fn atmark(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    classes::argument(args, "socket_atmark")?;
    Err(NativeError::throw(
        "Phera\\NotSupportedException",
        "socket_atmark() requires the platform's urgent-data mark query",
    ))
}

fn last_error(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let number = if args::value(args, 0).is_null() {
        context
            .state()
            .get::<LastError>()
            .map_or(0, |error| error.0)
    } else {
        classes::argument(args, "socket_last_error")?.error.get()
    };
    Ok(Value::Int(i64::from(number)))
}

fn clear_error(context: &mut dyn Context, args: &[Value]) -> NativeResult {
    if args::value(args, 0).is_null() {
        context.state().take::<LastError>();
    } else {
        classes::argument(args, "socket_clear_error")?.error.set(0);
    }
    Ok(Value::Null)
}

fn strerror(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let code = args::int(args, 0).unwrap_or_default() as i32;
    Ok(Value::from(error_text(code)))
}

pub(crate) fn error_text(code: i32) -> String {
    match code {
        -10001 => "Unknown host".into(),
        -10002 => "Host name lookup failure".into(),
        -10003 => "Unknown server error".into(),
        -10004 => "No address associated with name".into(),
        _ if code < -10000 => "Unknown resolver error".into(),
        0 => if cfg!(target_vendor = "apple") {
            "Undefined error: 0"
        } else {
            "Success"
        }
        .into(),
        _ => {
            let error = Errno::from_raw(code);
            if error == Errno::UnknownErrno {
                let colon = if cfg!(target_vendor = "apple") {
                    ":"
                } else {
                    ""
                };
                format!("Unknown error{colon} {code}")
            } else {
                error.desc().into()
            }
        }
    }
}

pub(crate) fn set_error(context: &mut dyn Context, handle: Option<&classes::Handle>, code: i32) {
    context.state().get_or_default::<LastError>().0 = code;
    if let Some(handle) = handle {
        handle.error.set(code);
    }
}

pub(crate) fn invalid_argument() -> std::io::Error {
    std::io::Error::from_raw_os_error(libc::EINVAL)
}
