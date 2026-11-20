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
        if crate::filters::is_filter_uri(from)
            || crate::filters::is_filter_uri(to)
            || crate::user_wrappers::locate(context, from).is_some()
            || crate::user_wrappers::locate(context, to).is_some()
        {
            return copy_wrapped(context, from, to, args::value(&arguments, 2)).await;
        }
        let source = (!from.is_empty())
            .then(|| resolve_file(context, from).ok())
            .flatten();
        let dest = (!to.is_empty())
            .then(|| resolve_file(context, to).ok())
            .flatten();
        if let Some(path) = source.as_ref()
            && path.is_dir()
        {
            return warn_false(
                context,
                "copy(): The first argument to copy() function cannot be a directory".into(),
            );
        }
        if let Some(path) = dest.as_ref()
            && path.is_dir()
        {
            return warn_false(
                context,
                "copy(): The second argument to copy() function cannot be a directory".into(),
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
        let result = IoPolicy::Blocking
            .run(move || {
                loop {
                    let bytes = match source.read(Some(CHUNK as u64)) {
                        Ok(bytes) if bytes.is_empty() => {
                            return dest.finish().map_err(|error| (error, None));
                        }
                        Ok(bytes) => bytes,
                        Err(error) => return Err((error, None)),
                    };
                    match write_bytes(&mut dest, &bytes) {
                        Ok(count) if count == bytes.len() => (),
                        Ok(_) => return Err((io::ErrorKind::WriteZero.into(), Some(bytes))),
                        Err(error) => return Err((error, Some(bytes))),
                    }
                }
            })
            .await?;
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
        let source = crate::user_wrappers::locate(context, from);
        let destination = crate::user_wrappers::locate(context, to);
        match (source, destination) {
            (Some(source), Some(destination)) if source.scheme == destination.scheme => {
                let supplied = args::value(&arguments, 2);
                let supplied = if supplied.is_null() {
                    crate::context_options::default_for_wrapper(context)
                } else {
                    supplied.clone()
                };
                return crate::user_paths::method(
                    context,
                    source,
                    function,
                    "rename",
                    vec![crate::user_paths::path(from), crate::user_paths::path(to)],
                    supplied,
                )
                .await;
            }
            (None, None) => (),
            _ => {
                return warn_false(
                    context,
                    "rename(): Cannot rename a file across wrapper types".into(),
                );
            }
        }
        if std::mem::discriminant(&Target::parse(from))
            != std::mem::discriminant(&Target::parse(to))
        {
            return warn_false(
                context,
                "rename(): Cannot rename a file across wrapper types".into(),
            );
        }
        let paths = local(context, function, from)
            .and_then(|source| local(context, function, to).map(|dest| (source, dest)));
        let result = match paths {
            Ok((source, dest)) => {
                IoPolicy::Blocking
                    .run(move || rename_path(&source, &dest))
                    .await?
            }
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) => warn_false(
                context,
                format!(
                    "rename({},{}): {}",
                    String::from_utf8_lossy(from),
                    String::from_utf8_lossy(to),
                    reason(&error)
                ),
            ),
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
            temporary
                .persist(destination)
                .map_err(|error| error.error)?;
            fs::remove_file(source)
        }
        other => other,
    }
}

fn endpoints<'a>(
    arguments: &'a [Value],
    function: &str,
) -> Result<(&'a [u8], &'a [u8]), phera_extension_api::NativeError> {
    let from = args::bytes(arguments, 0);
    let to = args::bytes(arguments, 1);
    check_path(function, "#1 ($from)", from)?;
    check_path(function, "#2 ($to)", to)?;
    validate(args::value(arguments, 2), function, 3)?;
    Ok((from, to))
}

async fn copy_wrapped(
    context: &mut dyn Context,
    from: &[u8],
    to: &[u8],
    supplied: &Value,
) -> NativeResult {
    let function = "copy";
    for (path, flags, side) in [(from, 0, "first"), (to, 2, "second")] {
        if let Some(wrapper) = crate::user_wrappers::locate(context, path) {
            if !wrapper.permitted(context, function) {
                return Ok(Value::Bool(false));
            }
            let object = wrapper.object(context, supplied.clone()).await?;
            let result = crate::user_file::invoke(
                context,
                &object,
                "url_stat",
                vec![crate::user_paths::path(path), Value::Int(flags)],
            )
            .await?;
            if let Some(Value::Array(info)) = result {
                let fields = crate::user_stat::fields(context, &info)?;
                if fields[2] & 0o170000 == 0o040000 {
                    return warn_false(
                        context,
                        format!(
                            "copy(): The {side} argument to copy() function cannot be a directory"
                        ),
                    );
                }
            } else if result.is_none() {
                crate::user_file::missing(context, &object, function, "url_stat", "");
            }
        }
    }
    if from == to {
        return Ok(Value::Bool(false));
    }
    let source = open_copy_resource(context, from, b"rb", supplied).await?;
    let Some(Value::Resource(source)) = source else {
        return Ok(Value::Bool(false));
    };
    let dest = open_copy_resource(context, to, b"wb", supplied).await;
    let result = match dest {
        Ok(Some(Value::Resource(ref dest))) => {
            crate::stream_copy::copy(context, &source, dest, function, u64::MAX).await
        }
        _ => Ok(Value::Bool(false)),
    };
    let source_closed =
        crate::handles::close_arguments(context, vec![Value::Resource(source)], "fclose").await;
    let dest_closed = match dest {
        Ok(Some(value @ Value::Resource(_))) => {
            crate::handles::close_arguments(context, vec![value], "fclose").await
        }
        Ok(_) => Ok(Value::Null),
        Err(error) => Err(error),
    };
    source_closed?;
    dest_closed?;
    Ok(Value::Bool(matches!(result?, Value::Int(_))))
}

async fn open_copy_resource(
    context: &mut dyn Context,
    path: &[u8],
    mode: &[u8],
    supplied: &Value,
) -> Result<Option<Value>, phera_extension_api::NativeError> {
    if let Some(wrapper) = crate::user_wrappers::locate(context, path) {
        return Ok(crate::user_file::open(
            context,
            wrapper,
            "copy",
            path,
            mode,
            0,
            supplied.clone(),
        )
        .await?
        .map(|file| crate::user_resource::make_file(context, file)));
    }
    if crate::filters::is_filter_uri(path)
        && let Some(stream) =
            crate::filters::open_request_filter(context, "copy", path, mode, false, supplied)
                .await?
    {
        return Ok(Some(stream));
    }
    Ok(open(context, "copy", path, mode, false)
        .await?
        .map(|file| crate::request::new_resource(context.state(), "stream", file)))
}
