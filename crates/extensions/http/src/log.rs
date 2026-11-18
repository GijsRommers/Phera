//! `error_log('saved')` writes to the SAPI log. Type 3 appends the exact
//! message bytes to a named file.

use std::io::Write;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};

use crate::request::{Sapi, sapi};

const PARAMS: &[Param] = &[
    Param::new("message", ParamType::String),
    Param::new("message_type", ParamType::Int).with_default(ConstantValue::Int(0)),
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
            3 => phera_ext_streams::append_log(context, destination, message).await,
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
