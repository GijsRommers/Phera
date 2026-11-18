//! `error_log('saved')` writes to the SAPI log. Type 3 appends the exact
//! message bytes to a named file.

use std::{io::Write, path::PathBuf};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};
use tokio::io::AsyncWriteExt;

use crate::request::{Sapi, sapi};

const PARAMS: &[Param] = &[
    Param::new("message", ParamType::String),
    Param::new("message_type", ParamType::Int).optional(),
    Param::new("destination", ParamType::String)
        .nullable()
        .optional(),
    Param::new("additional_headers", ParamType::String)
        .nullable()
        .optional(),
];

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous("error_log", PARAMS, error_log));
}

fn error_log(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let message = args::bytes(&arguments, 0);
        let destination = args::string(&arguments, 2);
        if destination.is_some_and(|name| name.as_bytes().contains(&0)) {
            return Err(NativeError::value_error(
                "error_log(): Argument #3 ($destination) must not contain any null bytes",
            ));
        }
        let destination = destination.map(|name| name.as_bytes()).unwrap_or_default();
        match args::int(&arguments, 1).unwrap_or(0) as i32 {
            1 => {
                context.warning("error_log(): E-mail logging is not supported by Phera".into());
                Ok(Value::Bool(false))
            }
            2 => Err(NativeError::value_error(
                "TCP/IP option is not available for error logging",
            )),
            3 => append(context, destination, message).await,
            _ => {
                let prefix = match sapi(context) {
                    Sapi::Cli => b"".as_slice(),
                    Sapi::Web => b"phera-server: ",
                };
                let message = message.split(|&byte| byte == 0).next().unwrap_or_default();
                let line = [prefix, message, b"\n"].concat();
                let result =
                    tokio::task::spawn_blocking(move || std::io::stderr().write_all(&line)).await;
                Ok(Value::Bool(result.is_ok_and(|result| result.is_ok())))
            }
        }
    })
}

async fn append(context: &mut dyn Context, destination: &[u8], message: &[u8]) -> NativeResult {
    if destination.is_empty() {
        return Err(NativeError::value_error("Path must not be empty"));
    }
    if destination.windows(3).any(|window| window == b"://") {
        context.warning("error_log(): File stream wrappers are not supported by Phera".into());
        return Ok(Value::Bool(false));
    }
    let path = file_path(destination);
    let mut file = match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
    {
        Ok(file) => file,
        Err(error) => {
            let destination = String::from_utf8_lossy(destination);
            let message = error.to_string();
            let error = message.split(" (os error").next().unwrap_or(&message);
            context.warning(format!(
                "error_log({destination}): Failed to open stream: {error}"
            ));
            return Ok(Value::Bool(false));
        }
    };
    Ok(Value::Bool(
        file.write_all(message).await.is_ok() && file.flush().await.is_ok(),
    ))
}

#[cfg(unix)]
fn file_path(bytes: &[u8]) -> PathBuf {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes).into()
}

#[cfg(not(unix))]
fn file_path(bytes: &[u8]) -> PathBuf {
    String::from_utf8_lossy(bytes).as_ref().into()
}
