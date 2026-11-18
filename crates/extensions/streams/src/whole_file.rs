//! `file_get_contents('report.txt', offset: -4)` reads the last four bytes.
//!
//! `file('report.txt', FILE_IGNORE_NEW_LINES)` returns its lines as an array.

use std::io::SeekFrom;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args, value::to_php_string,
};
use rustix::fs::{FlockOperation, flock};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, nonnegative, warn_false};
use crate::handles::{FILENAME, INCLUDE_PATH, LENGTH, read_notice};
use crate::io_policy::{CHUNK, IoPolicy};
use crate::resource::stream_argument;
use crate::stream::{Stream, Wrapper};
use crate::transfer::{finish_write, read, write_bytes};
use crate::wrapper::open;

pub const USE_PATH: i64 = 1;
pub const IGNORE_ENDINGS: i64 = 2;
pub const SKIP_EMPTY: i64 = 4;
pub const APPEND: i64 = 8;
pub const NO_DEFAULT_CONTEXT: i64 = 16;
const LOCK_EX: i64 = 2;

pub fn register(registry: &mut Registry) {
    const FLAGS: Param = Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(0));
    const OPEN_PARAMETERS: &[Param] = &[FILENAME, INCLUDE_PATH, OPTIONAL_CONTEXT];
    for entry in const {
        [
            FunctionEntry::asynchronous(
                "file_get_contents",
                const {
                    &[
                        FILENAME,
                        INCLUDE_PATH,
                        OPTIONAL_CONTEXT,
                        Param::new("offset", ParamType::Int).with_default(ConstantValue::Int(0)),
                        LENGTH,
                    ]
                },
                file_get_contents,
            ),
            FunctionEntry::asynchronous("readfile", OPEN_PARAMETERS, readfile),
            FunctionEntry::asynchronous(
                "file",
                const { &[FILENAME, FLAGS, OPTIONAL_CONTEXT] },
                file,
            ),
            FunctionEntry::asynchronous(
                "file_put_contents",
                const {
                    &[
                        FILENAME,
                        Param::new("data", ParamType::Mixed),
                        FLAGS,
                        OPTIONAL_CONTEXT,
                    ]
                },
                file_put_contents,
            ),
        ]
    } {
        registry.function(entry);
    }
    for (name, value) in [
        ("FILE_USE_INCLUDE_PATH", USE_PATH),
        ("FILE_IGNORE_NEW_LINES", IGNORE_ENDINGS),
        ("FILE_SKIP_EMPTY_LINES", SKIP_EMPTY),
        ("FILE_APPEND", APPEND),
        ("FILE_NO_DEFAULT_CONTEXT", NO_DEFAULT_CONTEXT),
    ] {
        registry.constant(name, value);
    }
}

fn file_get_contents(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "file_get_contents";
        let name = name(&arguments, function, 2)?;
        let length = args::int(&arguments, 4)
            .map(|length| nonnegative(function, "#5 ($length)", length))
            .transpose()?;
        let offset = args::int(&arguments, 3).unwrap_or(0);
        let use_include = args::bool(&arguments, 1).unwrap_or(false);
        let Some(mut stream) = open(context, function, name, b"rb", use_include).await? else {
            return Ok(Value::Bool(false));
        };
        if offset != 0 {
            let seek = if offset > 0 {
                SeekFrom::Start(offset as u64)
            } else {
                SeekFrom::End(offset)
            };
            if stream.seek(seek).is_err() {
                if !stream.seekable {
                    context.warning(format!("{function}(): Stream does not support seeking"));
                }
                return warn_false(
                    context,
                    format!("{function}(): Failed to seek to position {offset} in the stream"),
                );
            }
        }
        Ok(Value::from(
            collect(context, stream, function, length).await?,
        ))
    })
}

fn readfile(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "readfile";
        let name = name(&arguments, function, 2)?;
        let use_include = args::bool(&arguments, 1).unwrap_or(false);
        let Some(stream) = open(context, function, name, b"rb", use_include).await? else {
            return Ok(Value::Bool(false));
        };
        let result = stream
            .read_policy(None)
            .run(move || {
                let mut stream = stream;
                stream.read(None)
            })
            .await?;
        match result {
            Ok(bytes) => {
                context.write(&bytes);
                Ok(Value::Int(bytes.len() as i64))
            }
            Err(error) => {
                read_notice(context, function, &error, CHUNK as u64);
                Ok(Value::Int(-1))
            }
        }
    })
}

fn file(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = name(&arguments, "file", 2)?;
        let flags = args::int(&arguments, 1).unwrap_or(0);
        if flags & !(USE_PATH | IGNORE_ENDINGS | SKIP_EMPTY | NO_DEFAULT_CONTEXT) != 0 {
            return Err(NativeError::value_error(
                "file(): Argument #2 ($flags) must be a valid flag value",
            ));
        }
        let Some(stream) = open(context, "file", name, b"rb", flags & USE_PATH != 0).await? else {
            return Ok(Value::Bool(false));
        };
        let bytes = collect(context, stream, "file", None).await?;
        let mut lines = PhpArray::new();
        for mut line in bytes.split_inclusive(|&byte| byte == b'\n') {
            if flags & IGNORE_ENDINGS != 0
                && let Some(text) = line.strip_suffix(b"\n")
            {
                line = text.strip_suffix(b"\r").unwrap_or(text);
                if line.is_empty() && flags & SKIP_EMPTY != 0 {
                    continue;
                }
            }
            lines.push(Value::from(line.to_vec()))?;
        }
        Ok(Value::from(lines))
    })
}

fn name<'a>(
    arguments: &'a [Value],
    function: &str,
    context: usize,
) -> Result<&'a [u8], NativeError> {
    let name = args::bytes(arguments, 0);
    check_path(function, "#1 ($filename)", name)?;
    validate(args::value(arguments, context), function, context + 1)?;
    Ok(name)
}

async fn collect(
    context: &mut dyn Context,
    mut stream: Stream,
    function: &str,
    length: Option<u64>,
) -> Result<Vec<u8>, NativeError> {
    let amount = if function == "file_get_contents" {
        let default = stream.metadata().ok().map_or(CHUNK as u64, |meta| {
            meta.len()
                .saturating_sub(stream.position)
                .saturating_add(CHUNK as u64)
        });
        length.map_or(default, |len| len.min(default))
    } else {
        CHUNK as u64
    };
    match stream
        .read_policy(length)
        .run(move || stream.read(length))
        .await?
    {
        Ok(bytes) => Ok(bytes),
        Err(error) => {
            read_notice(context, function, &error, amount);
            Ok(Vec::new())
        }
    }
}

fn file_put_contents(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "file_put_contents";
        let name = name(&arguments, function, 3)?;
        let flags = args::int(&arguments, 2).unwrap_or(0);
        let data = args::value(&arguments, 1);
        let source = if matches!(data, Value::Resource(_)) {
            Some(stream_argument(data, function, "#2 ($data)")?)
        } else {
            None
        };
        let lock = flags & LOCK_EX != 0;
        let mode = if flags & APPEND != 0 {
            b"ab"
        } else if lock {
            if name.windows(3).any(|slice| slice == b"://")
                && !name
                    .get(..7)
                    .is_some_and(|part| part.eq_ignore_ascii_case(b"file://"))
            {
                return warn_false(
                    context,
                    "file_put_contents(): Exclusive locks may only be set for regular files".into(),
                );
            }
            b"cb"
        } else {
            b"wb"
        };
        let Some(mut stream) = open(context, function, name, mode, flags & USE_PATH != 0).await?
        else {
            return Ok(Value::Bool(false));
        };
        if lock {
            let (locked, restored) = IoPolicy::Blocking
                .run(move || {
                    let locked = stream
                        .descriptor()
                        .is_some_and(|file| flock(file, FlockOperation::LockExclusive).is_ok());
                    if locked && mode == b"cb" {
                        let _ = stream.truncate(0);
                    }
                    (locked, stream)
                })
                .await?;
            stream = restored;
            if !locked {
                return warn_false(
                    context,
                    "file_put_contents(): Exclusive locks are not supported for this stream".into(),
                );
            }
        }
        let bytes = if let Some(resource) = source {
            match read(resource, None).await? {
                Ok(bytes) => bytes,
                Err(error) => {
                    read_notice(context, function, &error, CHUNK as u64);
                    return Ok(Value::Bool(false));
                }
            }
        } else {
            let Some(bytes) = join_data(context, data)? else {
                return Ok(Value::Bool(false));
            };
            bytes
        };
        let size = bytes.len();
        let wrapper = stream.wrapper;
        let policy = stream.write_policy(size as u64);
        let (bytes, result) = if wrapper == Wrapper::Output {
            let result = stream.write(&bytes);
            (bytes, result)
        } else {
            policy
                .run(move || {
                    let result = write_bytes(&mut stream, &bytes);
                    (bytes, result)
                })
                .await?
        };
        if let Ok(count) = result
            && count != size
        {
            return warn_false(
                context,
                format!(
                    "{}(): Only {count} of {size} bytes written, possibly out of free disk space",
                    function
                ),
            );
        }
        finish_write(context, wrapper, &bytes, function, result)
    })
}

fn join_data(context: &mut dyn Context, data: &Value) -> Result<Option<Vec<u8>>, NativeError> {
    let mut bytes = Vec::new();
    match data {
        Value::Array(parts) => {
            for (_, field) in parts.iter() {
                bytes.extend_from_slice(to_php_string(field, context.diagnostics())?.as_bytes());
            }
        }
        Value::Object(_) => return Ok(None),
        value => bytes.extend_from_slice(to_php_string(value, context.diagnostics())?.as_bytes()),
    }
    Ok(Some(bytes))
}
