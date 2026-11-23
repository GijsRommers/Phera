//! The zlib file aliases use the same resource, cursor, and I/O as PHP streams.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use super::{GzipOptions, open_gzip};
use crate::errors::check_path;
use crate::handles::{self, FILENAME, INCLUDE_PATH, LENGTH, STREAM};
use crate::io_policy::{CHUNK, IoPolicy};
use crate::request::new_resource;
use crate::resource::{stream_arg, with_stream};
use crate::stream::{Stream, Wrapper};
use crate::transfer;

const FILE: &[Param] = &[FILENAME, INCLUDE_PATH];
const HANDLE: &[Param] = &[STREAM];
const WRITE: &[Param] = &[STREAM, Param::new("data", ParamType::String), LENGTH];

pub fn register(registry: &mut Registry) {
    const FUNCTIONS: &[(FunctionEntry, &str)] = &[
        (
            FunctionEntry::asynchronous(
                "gzopen",
                &[
                    FILENAME,
                    Param::new("mode", ParamType::String),
                    INCLUDE_PATH,
                ],
                open,
            ),
            "",
        ),
        (
            FunctionEntry::asynchronous("gzfile", FILE, file),
            "array|false",
        ),
        (
            FunctionEntry::asynchronous("readgzfile", FILE, read_file),
            "int|false",
        ),
        (
            FunctionEntry::asynchronous("gzclose", HANDLE, close),
            "bool",
        ),
        (FunctionEntry::sync("gzeof", HANDLE, eof), "bool"),
        (FunctionEntry::sync("gztell", HANDLE, tell), "int|false"),
        (
            FunctionEntry::asynchronous("gzrewind", HANDLE, rewind),
            "bool",
        ),
        (
            FunctionEntry::asynchronous(
                "gzseek",
                &[
                    STREAM,
                    Param::new("offset", ParamType::Int),
                    Param::new("whence", ParamType::Int)
                        .with_constant_default("SEEK_SET", ConstantValue::Int(0)),
                ],
                seek,
            ),
            "int",
        ),
        (
            FunctionEntry::asynchronous(
                "gzread",
                &[STREAM, Param::new("length", ParamType::Int)],
                read,
            ),
            "string|false",
        ),
        (
            FunctionEntry::asynchronous("gzgetc", HANDLE, get_char),
            "string|false",
        ),
        (
            FunctionEntry::asynchronous("gzgets", &[STREAM, LENGTH], get_line),
            "string|false",
        ),
        (
            FunctionEntry::asynchronous("gzwrite", WRITE, write),
            "int|false",
        ),
        (
            FunctionEntry::asynchronous("gzputs", WRITE, puts),
            "int|false",
        ),
        (
            FunctionEntry::asynchronous("gzpassthru", HANDLE, passthrough),
            "int",
        ),
    ];
    for (entry, result) in FUNCTIONS {
        registry.function(*entry);
        if !result.is_empty() {
            registry.describe_function(entry.name, result, None);
        }
    }
}

async fn open_stream(
    context: &mut dyn Context,
    values: &[Value],
    function: &str,
    mode: Option<&[u8]>,
) -> Result<Option<Stream>, phera_extension_api::NativeError> {
    let path = args::bytes(values, 0);
    check_path(function, "#1 ($filename)", path)?;
    let include = usize::from(mode.is_none()) + 1;
    open_gzip(
        context,
        function,
        path,
        path,
        mode.unwrap_or_else(|| args::bytes(values, 1)),
        GzipOptions {
            include_path: args::bool(values, include).unwrap_or(false),
            level: None,
            wrapper: Wrapper::GzipDirect,
        },
    )
    .await
}

fn open(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = open_stream(context, &values, "gzopen", None).await?;
        Ok(stream.map_or(Value::Bool(false), |stream| {
            new_resource(context.state(), "stream", stream)
        }))
    })
}

fn file(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(mut stream) = open_stream(context, &values, "gzfile", Some(b"rb")).await? else {
            return Ok(Value::Bool(false));
        };
        let lines = IoPolicy::Blocking
            .run(move || {
                let mut lines = Vec::new();
                while let Ok(line) = stream.line(Some(8190)) {
                    if line.is_empty() {
                        break;
                    }
                    let length = line
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(line.len());
                    lines.push(line[..length].to_vec());
                }
                lines
            })
            .await?;
        let mut result = PhpArray::new();
        for line in lines {
            result.push(Value::from(line))?;
        }
        Ok(Value::from(result))
    })
}

fn read_file(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let Some(mut stream) = open_stream(context, &values, "readgzfile", Some(b"rb")).await?
        else {
            return Ok(Value::Bool(false));
        };
        let mut total = 0;
        loop {
            let (next, read) = IoPolicy::Blocking
                .run(move || {
                    let data = stream.read(Some(CHUNK as u64));
                    (stream, data)
                })
                .await?;
            stream = next;
            let Ok(read) = read else { break };
            if read.is_empty() {
                break;
            }
            total += read.len() as i64;
            context.write(&read);
        }
        Ok(Value::Int(total))
    })
}

fn read(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::fread_arguments(context, values, "gzread"))
}

fn get_char(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::fgetc_arguments(context, values, "gzgetc"))
}

fn get_line(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::fgets_arguments(context, values, "gzgets"))
}

fn write(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::write_arguments(context, values, "gzwrite"))
}

fn puts(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::write_arguments(context, values, "gzputs"))
}

fn passthrough(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(transfer::fpassthru_arguments(context, values, "gzpassthru"))
}

fn close(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(handles::close_arguments(context, values, "gzclose"))
}

fn eof(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_stream(stream_arg(values, "gzeof")?, |stream| {
        Value::Bool(stream.eof)
    })
}

fn tell(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_stream(stream_arg(values, "gztell")?, |stream| {
        Value::Int(stream.position as i64)
    })
}

fn seek(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(handles::seek_arguments(context, values, "gzseek"))
}

fn rewind(context: &mut dyn Context, values: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(handles::rewind_arguments(context, values, "gzrewind"))
}
