//! `copy('input.txt', 'output.txt')` transfers the contents into a new file.

use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, reason, warn_false};
use crate::handles::read_notice;
use crate::io_policy::{CHUNK, IoPolicy};
use crate::mutations::local;
use crate::transfer::{finish_write, write_bytes};
use crate::wrapper::{Target, open, resolve_file};

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("from", ParamType::String),
        Param::new("to", ParamType::String),
        OPTIONAL_CONTEXT,
    ];
    registry.function(FunctionEntry::asynchronous("copy", PARAMS, copy));
    registry.function(FunctionEntry::asynchronous("rename", PARAMS, rename));
}

fn copy(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (from, to) = endpoints(&arguments, "copy")?;
        let source = (!from.is_empty()).then(|| resolve_file(context, from).ok()).flatten();
        let dest = (!to.is_empty()).then(|| resolve_file(context, to).ok()).flatten();
        if let Some(path) = source.as_ref() && path.is_dir() {
            return warn_false(
                context, "copy(): The first argument to copy() function cannot be a directory".into()
            );
        }
        if let Some(path) = dest.as_ref() && path.is_dir() {
            return warn_false(
                context, "copy(): The second argument to copy() function cannot be a directory".into()
            );
        }
        if let (Some(source), Some(dest)) = (source, dest)
            && let (Ok(source), Ok(dest)) = (source.metadata(), dest.metadata())
            && (source.dev(), source.ino()) == (dest.dev(), dest.ino())
        {
            return Ok(Value::Bool(false));
        }
        let Some(mut source) = open(context, "copy", from, b"rb", false).await? else {
            return Ok(Value::Bool(false));
        };
        let Some(mut dest) = open(context, "copy", to, b"wb", false).await? else {
            return Ok(Value::Bool(false));
        };
        if dest.wrapper == crate::stream::Wrapper::Output {
            let data = IoPolicy::Blocking.run(move || source.read(None)).await?;
            return match data {
                Ok(data) => {
                    context.write(&data);
                    Ok(Value::Bool(true))
                }
                Err(error) => {
                    read_notice(context, "copy", &error, CHUNK as u64);
                    Ok(Value::Bool(false))
                }
            };
        }
        let wrapper = dest.wrapper;
        let result = IoPolicy::Blocking.run(move || {
            loop {
                let bytes = match source.read(Some(CHUNK as u64)) {
                    Ok(bytes) if bytes.is_empty() => return Ok(()),
                    Ok(bytes) => bytes,
                    Err(error) => return Err((error, None)),
                };
                match write_bytes(&mut dest, &bytes) {
                    Ok(count) if count == bytes.len() => (),
                    Ok(_) => return Err((io::ErrorKind::WriteZero.into(), Some(bytes))),
                    Err(error) => return Err((error, Some(bytes))),
                }
            }
        }).await?;
        match result {
            Ok(()) => Ok(Value::Bool(true)),
            Err((error, Some(bytes))) => finish_write(context, wrapper, &bytes, "copy", Err(error)),
            Err((error, None)) => {
                read_notice(context, "copy", &error, CHUNK as u64);
                Ok(Value::Bool(false))
            }
        }
    })
}

fn rename(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "rename";
        let (from, to) = endpoints(&arguments, function)?;
        if std::mem::discriminant(&Target::parse(from))
            != std::mem::discriminant(&Target::parse(to))
        {
            return warn_false(context, "rename(): Cannot rename a file across wrapper types".into());
        }
        let paths = local(context, function, from).and_then(|source| {
            local(context, function, to).map(|dest| (source, dest))
        });
        let result = match paths {
            Ok((source, dest)) => IoPolicy::Blocking.run(move || rename_path(&source, &dest)).await?,
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => warn_false(context, format!(
                "rename({},{}): {}", String::from_utf8_lossy(from),
                String::from_utf8_lossy(to), reason(&error)
            )),
        }
    })
}

fn rename_path(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::rename(source, destination) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            if !fs::symlink_metadata(source)?.is_file() {
                return Err(error);
            }
            let parent = destination.parent().ok_or(io::ErrorKind::NotFound)?;
            let temporary = tempfile::NamedTempFile::new_in(parent)?;
            fs::copy(source, temporary.path())?;
            temporary.persist(destination).map_err(|error| error.error)?;
            fs::remove_file(source)
        }
        other => other,
    }
}

fn endpoints<'a>(arguments: &'a [Value], function: &str)
    -> Result<(&'a [u8], &'a [u8]), phera_extension_api::NativeError>
{
    let from = args::bytes(arguments, 0);
    let to = args::bytes(arguments, 1);
    check_path(function, "#1 ($from)", from)?;
    check_path(function, "#2 ($to)", to)?;
    validate(args::value(arguments, 2), function, 3)?;
    Ok((from, to))
}
