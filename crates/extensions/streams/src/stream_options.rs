//! `stream_get_meta_data($stream)['seekable']` reports support for seeking.

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::to_php_string,
};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

use crate::errors::wrapper_warning;
use crate::handles::STREAM;
use crate::io_policy::IoPolicy;
use crate::resource::{argument_resource, operate, stream_arg, with_stream};
use crate::stream::{Backing, Stream, Wrapper};
use crate::wrapper::Target;

pub fn register(registry: &mut Registry) {
    const TIMEOUT: &[Param] = &[
        STREAM,
        Param::new("seconds", ParamType::Int),
        Param::new("microseconds", ParamType::Int).with_default(ConstantValue::Int(0)),
    ];
    const BUFFER: &[Param] = &[STREAM, Param::new("size", ParamType::Int)];
    const BLOCK: &[Param] = &[STREAM, Param::new("enable", ParamType::Bool)];
    for entry in [
        FunctionEntry::sync("stream_set_chunk_size", BUFFER, set_chunk_size),
        FunctionEntry::asynchronous("stream_supports_lock", &[STREAM], supports_lock),
        FunctionEntry::sync("stream_get_meta_data", &[STREAM], get_meta_data),
        FunctionEntry::sync("stream_is_local", &[STREAM], is_local),
        FunctionEntry::sync("stream_isatty", &[STREAM], isatty),
        FunctionEntry::asynchronous("stream_set_blocking", BLOCK, set_blocking),
        FunctionEntry::asynchronous("socket_set_blocking", BLOCK, set_blocking),
        FunctionEntry::asynchronous("socket_set_timeout", TIMEOUT, set_timeout),
        FunctionEntry::sync("socket_get_status", &[STREAM], get_meta_data),
        FunctionEntry::asynchronous("stream_set_timeout", TIMEOUT, set_timeout),
        FunctionEntry::asynchronous("stream_set_read_buffer", BUFFER, set_read_buffer),
        FunctionEntry::asynchronous("stream_set_write_buffer", BUFFER, set_write_buffer),
    ] {
        registry.function(entry);
    }
}

fn is_local(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = args::value(arguments, 0);
    if let Value::Resource(resource) = value {
        if let Some(dir) = crate::user_directory::of(resource) {
            return Ok(Value::Bool(!dir.remote));
        }
        if crate::directory::is_open(resource) {
            return Ok(Value::Bool(true));
        }
        if let Some(file) = crate::user_file::resource_file(resource) {
            return Ok(Value::Bool(!file.remote));
        }
        return with_stream(
            stream_arg(arguments, "stream_is_local").map_err(|_| {
                NativeError::type_error(
                    "stream_is_local(): supplied resource is not a valid stream resource",
                )
            })?,
            |stream| {
                Value::Bool(!matches!(
                    stream.wrapper,
                    Wrapper::Data | Wrapper::GzipDirect | Wrapper::Gzip | Wrapper::Socket
                ))
            },
        );
    }
    let name = to_php_string(value, context.diagnostics())?;
    let name = name
        .as_bytes()
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
    if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
        return Ok(Value::Bool(!wrapper.remote));
    }
    let target = Target::parse(name);
    if let Target::Unavailable(scheme) = target {
        if [
            b"http".as_slice(),
            b"https",
            b"ftp",
            b"ftps",
            b"compress.zlib",
            b"phar",
            b"glob",
        ]
        .iter()
        .any(|name| scheme.eq_ignore_ascii_case(name))
        {
            return Ok(Value::Bool(false));
        }
        wrapper_warning(context, "stream_is_local", scheme);
        return Ok(Value::Bool(true));
    }
    Ok(Value::Bool(matches!(
        target,
        Target::File(_) | Target::Php(_) | Target::Gzip(_)
    )))
}

fn isatty(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if let Value::Resource(resource) = args::value(arguments, 0)
        && (crate::directory::is_open(resource)
            || crate::user_file::resource_file(resource).is_some())
    {
        return Ok(Value::Bool(false));
    }
    with_stream(stream_arg(arguments, "stream_isatty")?, |stream| {
        Value::Bool(stream.descriptor().is_some_and(rustix::termios::isatty))
    })
}

fn get_meta_data(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let name = "stream_get_meta_data";
    let resource = argument_resource(args::value(arguments, 0), name, "#1 ($stream)")?;
    if crate::directory::is_open(resource) {
        let mut data = PhpArray::new();
        let user = crate::user_directory::of(resource);
        if let Some(dir) = &user {
            for (key, value) in [
                ("timed_out", Value::Bool(false)),
                ("blocked", Value::Bool(true)),
                ("eof", Value::Bool(false)),
                ("wrapper_data", Value::Object(dir.object.clone())),
                ("wrapper_type", Value::from("user-space")),
                ("stream_type", Value::from("user-space-dir")),
                ("mode", Value::from("r")),
                ("unread_bytes", Value::Int(0)),
                ("seekable", Value::Bool(true)),
            ] {
                data.insert(ArrayKey::str(key), value);
            }
            return Ok(Value::from(data));
        }
        for (key, value) in [
            ("timed_out", Value::Bool(false)),
            ("blocked", Value::Bool(true)),
            ("eof", Value::Bool(false)),
            ("wrapper_type", Value::from("plainfile")),
            ("stream_type", Value::from("dir")),
            ("mode", Value::from("r")),
            ("unread_bytes", Value::Int(0)),
            ("seekable", Value::Bool(true)),
        ] {
            data.insert(ArrayKey::str(key), value);
        }
        return Ok(Value::from(data));
    }
    let stream = stream_arg(arguments, name)?;
    if let Some(file) = crate::user_file::resource_file(stream) {
        return Ok(file.metadata());
    }
    with_stream(stream, |stream| Value::from(metadata(stream)))
}

fn metadata(stream: &Stream) -> PhpArray {
    let mut result = PhpArray::new();
    let mut insert = |key: &str, value| {
        result.insert(ArrayKey::str(key), value);
    };
    if let Backing::Socket(socket) = &stream.backing {
        insert("timed_out", Value::Bool(socket.timed_out()));
        insert("blocked", Value::Bool(stream.blocked));
        insert("eof", Value::Bool(socket.eof()));
        insert("stream_type", Value::from(socket.stream_type()));
        insert("mode", Value::from("r+"));
        insert("unread_bytes", Value::Int(socket.unread() as i64));
        insert("seekable", Value::Bool(false));
        if !stream.uri.is_empty() {
            insert("uri", Value::from(stream.uri.clone()));
        }
        return result;
    }
    match stream.wrapper {
        Wrapper::Data => {
            let header = stream.data_header.as_deref().unwrap_or_default();
            let mut parts = header.split(|byte| *byte == b';');
            if let Some(media) = parts.next().filter(|part| !part.is_empty()) {
                insert("mediatype", Value::from(media.to_vec()));
            }
            for field in parts {
                if let Some(index) = field.iter().position(|byte| *byte == b'=')
                    && &field[..index] != b"mediatype"
                {
                    insert(
                        &String::from_utf8_lossy(&field[..index]),
                        Value::from(field[index + 1..].to_vec()),
                    );
                }
            }
            insert("base64", Value::Bool(header.ends_with(b";base64")));
        }
        Wrapper::Temp if stream.spill_limit.is_some() => (),
        _ => {
            insert("timed_out", Value::Bool(false));
            insert("blocked", Value::Bool(stream.blocked));
            insert("eof", Value::Bool(stream.eof));
        }
    }
    let (wrapper, kind) = match stream.wrapper {
        Wrapper::File => ("plainfile", "STDIO"),
        Wrapper::Gzip | Wrapper::GzipDirect => ("ZLIB", "ZLIB"),
        Wrapper::Standard => ("PHP", "STDIO"),
        Wrapper::Process => ("plainfile", "STDIO"),
        Wrapper::ProcessSocket | Wrapper::Socket => ("", "generic_socket"),
        Wrapper::Memory => ("PHP", "MEMORY"),
        Wrapper::Temp => ("PHP", "TEMP"),
        Wrapper::Data => ("RFC2397", "RFC2397"),
        Wrapper::Input => ("PHP", "Input"),
        Wrapper::Output => ("PHP", "Output"),
    };
    if stream.wrapper != Wrapper::GzipDirect && stream.child_pipe.is_none() {
        insert("wrapper_type", Value::from(wrapper));
    }
    insert("stream_type", Value::from(kind));
    insert("mode", Value::from(stream.mode.clone()));
    insert("unread_bytes", Value::Int(stream.unread() as i64));
    insert("seekable", Value::Bool(stream.seekable));
    if stream.wrapper != Wrapper::GzipDirect && stream.child_pipe.is_none() {
        insert("uri", Value::from(stream.uri.clone()));
    }
    result
}

fn set_blocking(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "stream_set_blocking")?;
        let enable = args::bool(&arguments, 1).unwrap_or(false);
        if let Some(file) = crate::user_file::resource_file(resource) {
            let success = file
                .option(
                    context,
                    "stream_set_blocking",
                    1,
                    i64::from(enable),
                    Value::Null,
                )
                .await?;
            return Ok(Value::Bool(success));
        }
        let value = operate(resource, IoPolicy::Inline, move |stream| {
            if stream.child_pipe.is_some() || stream.wrapper == Wrapper::Socket {
                stream.blocked = enable;
                return Ok(());
            }
            let Some(file) = stream.descriptor() else {
                return Ok(());
            };
            if stream.wrapper == Wrapper::Standard {
                // Standard descriptors are dup()s of a process-wide open file description.
                return Err(rustix::io::Errno::NOTSUP);
            }
            let mut flags = fcntl_getfl(file)?;
            flags.set(OFlags::NONBLOCK, !enable);
            fcntl_setfl(file, flags)?;
            stream.blocked = enable;
            Ok(())
        })
        .await?;
        Ok(Value::Bool(value.is_ok()))
    })
}

fn supports_lock(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_supports_lock";
        let resource = argument_resource(args::value(&arguments, 0), function, "#1 ($stream)")?;
        if crate::directory::is_open(resource) {
            return Ok(Value::Bool(false));
        }
        if let Some(file) = crate::user_file::resource_file(resource) {
            return Ok(Value::Bool(file.lock(context, function, 0).await?));
        }
        with_stream(stream_arg(&arguments, function)?, |stream| {
            Value::Bool(stream.lock_descriptor().is_some())
        })
    })
}

fn set_chunk_size(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let resource = stream_arg(arguments, "stream_set_chunk_size")?;
    let size = args::int(arguments, 1).unwrap_or_default();
    if size <= 0 {
        return Err(NativeError::value_error(
            "stream_set_chunk_size(): Argument #2 ($size) must be greater than 0",
        ));
    }
    let size = i32::try_from(size).map_err(|_| {
        NativeError::value_error("stream_set_chunk_size(): Argument #2 ($size) is too large")
    })?;
    if let Some(file) = crate::user_file::resource_file(resource) {
        return Ok(Value::Int(file.chunk.replace(size as usize) as i64));
    }
    with_stream(resource, |stream| {
        let previous = std::mem::replace(&mut stream.chunk_size, size as usize);
        Value::Int(previous as i64)
    })
}

fn set_timeout(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_set_timeout";
        let resource = stream_arg(&arguments, function)?;
        let seconds = args::int(&arguments, 1).unwrap_or_default();
        let microseconds = args::int(&arguments, 2).unwrap_or_default();
        let Some(file) = crate::user_file::resource_file(resource) else {
            return with_stream(resource, |stream| {
                let Backing::Socket(socket) = &stream.backing else {
                    return Value::Bool(false);
                };
                let micros = seconds
                    .saturating_mul(1_000_000)
                    .saturating_add(microseconds);
                let timeout = u64::try_from(micros)
                    .ok()
                    .map(std::time::Duration::from_micros);
                socket.set_timeout(timeout);
                Value::Bool(true)
            });
        };
        Ok(Value::Bool(
            file.option(context, function, 4, seconds, Value::Int(microseconds))
                .await?,
        ))
    })
}

fn set_write_buffer(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(set_buffer(context, arguments, Buffer::Write))
}

fn set_read_buffer(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(set_buffer(context, arguments, Buffer::Read))
}

enum Buffer {
    Read,
    Write,
}

async fn set_buffer(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    kind: Buffer,
) -> NativeResult {
    let (function, option) = match kind {
        Buffer::Read => ("stream_set_read_buffer", 2),
        Buffer::Write => ("stream_set_write_buffer", 3),
    };
    let resource = stream_arg(&arguments, function)?;
    let size = args::int(&arguments, 1).unwrap_or(0);
    if let Some(file) = crate::user_file::resource_file(resource) {
        let (mode, capacity) = if size == 0 {
            (0, crate::include_stream::platform_buffer_size())
        } else {
            (2, size)
        };
        let success = file
            .option(context, function, option, mode, Value::Int(capacity))
            .await?;
        return Ok(Value::Int(if success { 0 } else { -1 }));
    }
    match kind {
        Buffer::Read => with_stream(resource, |stream| {
            stream.read_buffered = size != 0;
            Value::Int(0)
        }),
        Buffer::Write => Ok(Value::Int(-1)),
    }
}
