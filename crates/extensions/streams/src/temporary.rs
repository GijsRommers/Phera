//! `tmpfile()` creates a stream whose path disappears when the stream closes.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
};
use tempfile::Builder;

use crate::errors::{check_path, reason};
use crate::io_policy::IoPolicy;
use crate::request::{bytes, directory as cwd, new_resource, path};
use crate::stream::{Stream, Wrapper};

pub fn register(registry: &mut Registry) {
    const TEMPNAM: &[Param] = &[
        Param::new("directory", ParamType::String),
        Param::new("prefix", ParamType::String),
    ];
    registry.function(FunctionEntry::asynchronous("tempnam", TEMPNAM, tempnam));
    registry.function(FunctionEntry::asynchronous("tmpfile", &[], tmpfile));
    registry.function(FunctionEntry::sync(
        "sys_get_temp_dir",
        &[],
        sys_get_temp_dir,
    ));
}

fn sys_get_temp_dir(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(bytes(&std::env::temp_dir()).to_vec()))
}

fn tmpfile(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let result = IoPolicy::Blocking
            .run(|| Builder::new().prefix("php").tempfile())
            .await?;
        match result {
            Ok(temp) => {
                let name = bytes(temp.path()).to_vec();
                let (file, path) = temp.into_parts();
                let mut stream = Stream::file(file, Wrapper::File, b"w+b", &name);
                stream.temporary_path = Some(path);
                Ok(new_resource(context.state(), "stream", stream))
            }
            Err(error) => {
                context.warning(format!("tmpfile(): {}", reason(&error)));
                Ok(Value::Bool(false))
            }
        }
    })
}

fn tempnam(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let directory = args::bytes(&arguments, 0);
        check_path("tempnam", "#1 ($directory)", directory)?;
        let prefix = args::bytes(&arguments, 1);
        check_path("tempnam", "#2 ($prefix)", prefix)?;
        let prefix = path(prefix).file_name().unwrap_or_default();
        let prefix = prefix.as_encoded_bytes();
        let prefix = path(&prefix[..prefix.len().min(63)]).as_os_str().to_owned();
        let directory = if directory.is_empty() {
            None
        } else {
            cwd(context).resolve(path(directory)).ok()
        };
        let (result, fallback) = IoPolicy::Blocking
            .run(move || {
                let mut builder = Builder::new();
                builder.prefix(&prefix);
                let created = directory
                    .and_then(|path| path.canonicalize().ok())
                    .and_then(|path| builder.tempfile_in(path).ok());
                if let Some(file) = created {
                    return (file.keep().ok(), false);
                }
                (
                    builder.tempfile().ok().and_then(|file| file.keep().ok()),
                    true,
                )
            })
            .await?;
        if fallback {
            context.notice("tempnam(): file created in the system's temporary directory".into());
        }
        Ok(result.map_or(Value::Bool(false), |(_, path)| {
            Value::from(bytes(&path).to_vec())
        }))
    })
}
