//! `posix_mkfifo('queue', 0600)` creates a named pipe in PHP's working directory.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::sys::stat::Mode;
use nix::unistd::{AccessFlags, access, mkfifo};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, WorkingDirectory, args,
};

const FILENAME: Param = Param::new("filename", ParamType::String).without_nul();

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("posix_getcwd", &[], cwd));
    registry.function(FunctionEntry::asynchronous(
        "posix_mkfifo",
        const { &[FILENAME, Param::new("permissions", ParamType::Int)] },
        fifo,
    ));
    registry.function(FunctionEntry::asynchronous(
        "posix_access",
        const {
            &[
                FILENAME,
                Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0)),
            ]
        },
        check,
    ));
}

fn cwd(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    let dir = context.state().get_or_default::<WorkingDirectory>();
    Ok(match dir.path() {
        Some(path) => Value::from(path.as_os_str().as_bytes()),
        None => super::fail(context, Errno::ENOENT),
    })
}

fn fifo(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let path = path(context, &arguments);
        let mode = Mode::from_bits_truncate(args::int(&arguments, 1).unwrap_or_default() as _);
        filesystem(context, move || mkfifo(&path.ok_or(Errno::ENOENT)?, mode)).await
    })
}

fn check(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let path = path(context, &arguments);
        let flags =
            AccessFlags::from_bits_retain(args::int(&arguments, 1).unwrap_or_default() as i32);
        filesystem(context, move || access(&path.ok_or(Errno::EIO)?, flags)).await
    })
}

fn path(context: &mut dyn Context, arguments: &[Value]) -> Option<PathBuf> {
    let name = args::bytes(arguments, 0);
    if name.is_empty() {
        return None;
    }
    let dir = context.state().get_or_default::<WorkingDirectory>();
    dir.resolve(Path::new(OsStr::from_bytes(name))).ok()
}

async fn filesystem(
    context: &mut dyn Context,
    operation: impl FnOnce() -> Result<(), Errno> + Send + 'static,
) -> NativeResult {
    let result = tokio::task::spawn_blocking(operation)
        .await
        .map_err(|error| NativeError::error(error.to_string()))?;
    Ok(match result {
        Ok(()) => Value::Bool(true),
        Err(error) => super::fail(context, error),
    })
}
