//! `popen('printf hello', 'r')` returns a stream whose child is reaped by `pclose`.

use std::io;
use std::os::fd::OwnedFd;
use std::process::Stdio;

use phera_ext_streams::{
    ChildStreamKind, CloseAttachment, child_stream, close_child_stream, set_attachment,
};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

use super::super::child::{ManagedChild, exit_code};
use super::super::command::{bounded, configure, reason, shell, spawn};

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous(
        "popen",
        const { &[super::COMMAND, Param::new("mode", ParamType::String)] },
        open,
    ));
    registry.function(FunctionEntry::asynchronous(
        "pclose",
        const { &[Param::new("handle", ParamType::Mixed)] },
        close,
    ));
}

fn open(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let web = super::super::permitted(context, "popen")?;
        let bytes = args::bytes(&arguments, 0);
        let mode = args::bytes(&arguments, 1);
        let mut posix_mode = mode.to_vec();
        if let Some(index) = posix_mode.iter().position(|&byte| byte == b'b') {
            posix_mode.remove(index);
        }
        let read = match posix_mode.as_slice() {
            b"r" => true,
            b"w" => false,
            b"" | b"rb" | b"wb" => {
                warning(context, bytes, &posix_mode, "Invalid argument");
                return Ok(Value::Bool(false));
            }
            _ => {
                return Err(NativeError::value_error(
                    "popen(): Argument #2 ($mode) must be one of \"r\", \"rb\", \"w\", or \"wb\"",
                ));
            }
        };
        let mut command = shell(bytes);
        configure(context, &mut command, None, &Value::Null, web).await?;
        let (reader, writer) = match io::pipe() {
            Ok(pipe) => pipe,
            Err(error) => {
                warning(context, bytes, &posix_mode, &reason(&error));
                return Ok(Value::Bool(false));
            }
        };
        let parent: OwnedFd = if read {
            command.stdout(Stdio::from(writer));
            reader.into()
        } else {
            command.stdin(Stdio::from(reader));
            writer.into()
        };
        let mut child = match spawn(context, command, web).await? {
            Ok(child) => child,
            Err(error) => {
                warning(context, bytes, &posix_mode, &reason(&error));
                return Ok(Value::Bool(false));
            }
        };
        let value = match child_stream(context, parent, mode, ChildStreamKind::Pipe) {
            Ok(value) => value,
            Err(error) => {
                warning(context, bytes, mode, &reason(&error));
                return Ok(Value::Bool(false));
            }
        };
        child.detach_on_cli_cleanup();
        if let Value::Resource(resource) = &value {
            set_attachment(resource, Box::new(child))?;
        }
        Ok(value)
    })
}

fn close(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { close_child_stream(context, args::value(&arguments, 0)).await })
}

impl CloseAttachment for ManagedChild {
    fn close(mut self: Box<Self>, context: &mut dyn Context) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let code = bounded(context, self.wait()).await?.map_or(-1, exit_code);
            Ok(Value::Int(code))
        })
    }
}

fn warning(context: &mut dyn Context, command: &[u8], mode: &[u8], message: &str) {
    let command = String::from_utf8_lossy(command);
    let mode = String::from_utf8_lossy(mode);
    context.warning(format!("popen({command},{mode}): {message}"));
}
