//! `stream_get_meta_data($stream)['seekable']` reports support for seeking.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args, value::to_php_string,
};
use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};

use crate::errors::wrapper_warning;
use crate::handles::STREAM;
use crate::io_policy::IoPolicy;
use crate::resource::{operate, stream_arg, with_stream};
use crate::stream::{Stream, Wrapper};
use crate::wrapper::Target;

pub fn register(registry: &mut Registry) {
    const TIMEOUT: &[Param] = &[
        STREAM,
        Param::new("seconds", ParamType::Int),
        Param::new("microseconds", ParamType::Int).optional(),
    ];
    const BUFFER: &[Param] = &[STREAM, Param::new("size", ParamType::Int)];
    const BLOCK: &[Param] = &[STREAM, Param::new("enable", ParamType::Bool)];
    for entry in [
        FunctionEntry::sync("stream_get_meta_data", &[STREAM], get_meta_data),
        FunctionEntry::sync("stream_get_wrappers", &[], get_wrappers),
        FunctionEntry::sync("stream_is_local", &[STREAM], is_local),
        FunctionEntry::sync("stream_isatty", &[STREAM], isatty),
        FunctionEntry::asynchronous("stream_set_blocking", BLOCK, set_blocking),
        FunctionEntry::sync("stream_set_timeout", TIMEOUT, set_timeout),
        FunctionEntry::sync("stream_set_read_buffer", BUFFER, set_read_buffer),
        FunctionEntry::sync("stream_set_write_buffer", BUFFER, set_write_buffer),
    ] {
        registry.function(entry);
    }
}

fn get_wrappers(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let mut wrappers = PhpArray::new();
    for name in ["php", "file", "data"] {
        wrappers.push(Value::from(name))?;
    }
    Ok(Value::from(wrappers))
}

fn is_local(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = args::value(arguments, 0);
    if value.is_resource() {
        return with_stream(stream_arg(arguments, "stream_is_local")?, |stream| {
            Value::Bool(stream.wrapper != Wrapper::Data)
        });
    }
    let name = to_php_string(value, context.diagnostics())?;
    let name = name
        .as_bytes()
        .split(|byte| *byte == 0)
        .next()
        .unwrap_or_default();
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
        Target::File(_) | Target::Php(_)
    )))
}

fn isatty(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    with_stream(stream_arg(arguments, "stream_isatty")?, |stream| {
        Value::Bool(stream.descriptor().is_some_and(rustix::termios::isatty))
    })
}

fn get_meta_data(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let stream = stream_arg(arguments, "stream_get_meta_data")?;
    with_stream(stream, |stream| Value::from(metadata(stream)))
}

fn metadata(stream: &Stream) -> PhpArray {
    let mut result = PhpArray::new();
    let mut insert = |key: &str, value| {
        result.insert(ArrayKey::str(key), value);
    };
    match stream.wrapper {
        Wrapper::Data => {
            let header = stream.data_header.as_deref().unwrap_or_default();
            let mut parts = header.split(|byte| *byte == b';');
            if let Some(media) = parts.next().filter(|part| !part.is_empty()) {
                insert("mediatype", Value::from(media.to_vec()));
            }
            for field in parts {
                if let Some(index) = field.iter().position(|byte| *byte == b'=') {
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
        Wrapper::Standard => ("PHP", "STDIO"),
        Wrapper::Memory => ("PHP", "MEMORY"),
        Wrapper::Temp => ("PHP", "TEMP"),
        Wrapper::Data => ("RFC2397", "RFC2397"),
        Wrapper::Input => ("PHP", "Input"),
        Wrapper::Output => ("PHP", "Output"),
    };
    insert("wrapper_type", Value::from(wrapper));
    insert("stream_type", Value::from(kind));
    insert("mode", Value::from(stream.mode.clone()));
    insert("unread_bytes", Value::Int(stream.unread() as i64));
    insert("seekable", Value::Bool(stream.seekable));
    insert("uri", Value::from(stream.uri.clone()));
    result
}

fn set_blocking(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "stream_set_blocking")?;
        let enable = args::bool(&arguments, 1).unwrap_or(false);
        let value = operate(resource, IoPolicy::Inline, move |stream| {
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

fn set_timeout(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    stream_arg(arguments, "stream_set_timeout")?;
    // Timeout options are reported as unsupported by PHP's non-socket streams.
    Ok(Value::Bool(false))
}

fn set_write_buffer(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    stream_arg(arguments, "stream_set_write_buffer")?;
    // These streams write directly to their descriptors or backing buffers.
    Ok(Value::Int(-1))
}

fn set_read_buffer(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let resource = stream_arg(arguments, "stream_set_read_buffer")?;
    with_stream(resource, |stream| {
        stream.read_buffered = args::int(arguments, 1).unwrap_or(0) != 0;
        Value::Int(0)
    })
}
