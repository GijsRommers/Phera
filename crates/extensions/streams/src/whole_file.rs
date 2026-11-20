//! `file_get_contents('report.txt', offset: -4)` reads the last four bytes.
//!
//! `file('report.txt', FILE_IGNORE_NEW_LINES)` returns its lines as an array.

use std::io::SeekFrom;

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, nonnegative, warn_false};
use crate::handles::{FILENAME, INCLUDE_PATH, LENGTH, read_notice};
use crate::io_policy::CHUNK;
use crate::request::files;
use crate::stream::Stream;
use crate::whole_write::file_put_contents;
use crate::wrapper::open;

pub const USE_PATH: i64 = 1;
pub const IGNORE_ENDINGS: i64 = 2;
pub const SKIP_EMPTY: i64 = 4;
pub const APPEND: i64 = 8;
pub const NO_DEFAULT_CONTEXT: i64 = 16;

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
        files(context).use_default_context(args::value(&arguments, 2));
        if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
            return user_contents(
                context,
                (wrapper, name),
                function,
                use_include,
                args::value(&arguments, 2),
                (offset, length),
                true,
            )
            .await;
        }
        let Some(mut stream) = open(context, function, name, b"rb", use_include).await? else {
            return Ok(Value::Bool(false));
        };
        files(context).claim_id();
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
        files(context).use_default_context(args::value(&arguments, 2));
        if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
            let explicit = args::value(&arguments, 2);
            let supplied = if explicit.is_null() {
                crate::context_options::default_for_wrapper(context)
            } else {
                explicit.clone()
            };
            let flags = if use_include {
                crate::user_wrappers::USE_PATH
            } else {
                0
            };
            let Some(file) =
                crate::user_file::open(context, wrapper, function, name, b"rb", flags, supplied)
                    .await?
            else {
                return Ok(Value::Bool(false));
            };
            files(context).claim_id();
            let result = user_passthru(context, &file, function).await;
            let closed = file.close(context).await;
            let result = result?;
            closed?;
            return Ok(result);
        }
        let Some(stream) = open(context, function, name, b"rb", use_include).await? else {
            return Ok(Value::Bool(false));
        };
        files(context).claim_id();
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
        if flags & NO_DEFAULT_CONTEXT == 0 {
            files(context).use_default_context(args::value(&arguments, 2));
        }
        let use_path = flags & USE_PATH != 0;
        let use_default = flags & NO_DEFAULT_CONTEXT == 0;
        let bytes = if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
            match user_contents(
                context,
                (wrapper, name),
                "file",
                use_path,
                args::value(&arguments, 2),
                (0, None),
                use_default,
            )
            .await?
            {
                Value::String(bytes) => bytes,
                other => return Ok(other),
            }
        } else {
            let Some(stream) = open(context, "file", name, b"rb", use_path).await? else {
                return Ok(Value::Bool(false));
            };
            files(context).claim_id();
            collect(context, stream, "file", None).await?.into()
        };
        let mut lines = PhpArray::new();
        for mut line in bytes.as_bytes().split_inclusive(|&byte| byte == b'\n') {
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

pub fn name<'a>(
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

async fn user_contents(
    context: &mut dyn Context,
    (wrapper, name): (crate::user_wrappers::UserWrapper, &[u8]),
    function: &str,
    use_include: bool,
    supplied_context: &Value,
    (offset, length): (i64, Option<u64>),
    use_default: bool,
) -> NativeResult {
    let flags = if use_include {
        crate::user_wrappers::USE_PATH
    } else {
        0
    };
    let supplied_context = if supplied_context.is_null() && use_default {
        crate::context_options::default_for_wrapper(context)
    } else {
        supplied_context.clone()
    };
    let Some(file) = crate::user_file::open(
        context,
        wrapper,
        function,
        name,
        b"rb",
        flags,
        supplied_context,
    )
    .await?
    else {
        return Ok(Value::Bool(false));
    };
    files(context).claim_id();
    let result = async {
        if offset != 0
            && !file
                .seek(context, function, offset, if offset < 0 { 2 } else { 0 })
                .await?
        {
            return warn_false(
                context,
                format!("{function}(): Failed to seek to position {offset} in the stream"),
            );
        }
        file.collect(context, function, length)
            .await
            .map(Value::from)
    }
    .await;
    let closed = file.close(context).await;
    let result = result?;
    closed?;
    Ok(result)
}

async fn user_passthru(
    context: &mut dyn Context,
    file: &crate::user_file::UserFile,
    function: &str,
) -> NativeResult {
    let mut count = 0;
    loop {
        match file.read(context, function, CHUNK, false).await? {
            Ok(bytes) if bytes.is_empty() => break,
            Ok(bytes) => {
                count += bytes.len();
                context.write(&bytes);
            }
            Err(_) => return Ok(Value::Int(if count == 0 { -1 } else { count as i64 })),
        }
    }
    Ok(Value::Int(count as i64))
}
