//! `fgets($file)` includes its newline, and a short `fread` can set EOF.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
    value::Resource,
};
use std::io;

use crate::handles::{LENGTH, STREAM, positive, read_amount, read_notice, write_notice};
use crate::resource::{operate, stream_arg, with_stream};
use crate::stream::{Stream, Wrapper};

pub fn register(registry: &mut Registry) {
    const WRITE_ARGUMENTS: &[Param] = &[STREAM, Param::new("data", ParamType::String), LENGTH];
    for entry in const {
        [
            FunctionEntry::asynchronous(
                "fread",
                const { &[STREAM, Param::new("length", ParamType::Int)] },
                fread,
            ),
            FunctionEntry::asynchronous("fgetc", const { &[STREAM] }, fgetc),
            FunctionEntry::asynchronous("fgets", const { &[STREAM, LENGTH] }, fgets),
            FunctionEntry::asynchronous("fwrite", WRITE_ARGUMENTS, fwrite),
            FunctionEntry::asynchronous("fputs", WRITE_ARGUMENTS, fputs),
            FunctionEntry::asynchronous("fpassthru", const { &[STREAM] }, fpassthru),
        ]
    } {
        registry.function(entry);
    }
}

fn fread(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "fread")?;
        let length = positive("fread", args::int(&arguments, 1).unwrap_or(0))?;
        match read(resource, Some(length)).await? {
            Ok(bytes) => Ok(Value::from(bytes)),
            Err(error) => {
                read_notice(context, "fread", &error, read_amount(Some(length)));
                Ok(Value::Bool(false))
            }
        }
    })
}

fn fgetc(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "fgetc")?;
        match read(resource, Some(1)).await? {
            Ok(bytes) if !bytes.is_empty() => Ok(Value::from(bytes)),
            Ok(_) => Ok(Value::Bool(false)),
            Err(error) => {
                read_notice(context, "fgetc", &error, read_amount(Some(1)));
                Ok(Value::Bool(false))
            }
        }
    })
}

fn fgets(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "fgets")?;
        let length = args::int(&arguments, 1)
            .map(|length| positive("fgets", length).map(|length| length - 1))
            .transpose()?;
        match line(resource, length).await? {
            Ok(bytes) if !bytes.is_empty() => Ok(Value::from(bytes)),
            Ok(_) => Ok(Value::Bool(false)),
            Err(error) => {
                read_notice(context, "fgets", &error, read_amount(None));
                Ok(Value::Bool(false))
            }
        }
    })
}

pub async fn line(
    resource: &Resource,
    length: Option<u64>,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    let policy = with_stream(resource, |stream| stream.read_policy(length))?;
    operate(resource, policy, move |stream| stream.line(length)).await
}

pub async fn read(
    resource: &Resource,
    length: Option<u64>,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    let policy = with_stream(resource, |stream| stream.read_policy(length))?;
    operate(resource, policy, move |stream| stream.read(length)).await
}

fn fwrite(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(write_arguments(context, arguments, "fwrite"))
}

fn fputs(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(write_arguments(context, arguments, "fputs"))
}

async fn write_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let resource = stream_arg(&arguments, function)?;
    let data = args::bytes(&arguments, 1);
    let length = args::int(&arguments, 2).map_or(data.len(), |length| {
        usize::try_from(length.max(0))
            .unwrap_or(usize::MAX)
            .min(data.len())
    });
    write(context, resource, &data[..length], function).await
}

pub async fn write(
    context: &mut dyn Context,
    resource: &Resource,
    data: &[u8],
    function: &str,
) -> NativeResult {
    let length = data.len();
    let (wrapper, policy) = with_stream(resource, |stream| {
        (stream.wrapper, stream.write_policy(length as u64))
    })?;
    let result = match policy {
        crate::io_policy::IoPolicy::Inline => with_stream(resource, |stream| stream.write(data))?,
        policy if wrapper != Wrapper::Output => {
            let data = data.to_vec();
            operate(resource, policy, move |stream| write_bytes(stream, &data)).await?
        }
        _ => with_stream(resource, |stream| stream.write(data))?,
    };
    finish_write(context, wrapper, data, function, result)
}

pub fn finish_write(
    context: &mut dyn Context,
    wrapper: Wrapper,
    data: &[u8],
    function: &str,
    result: io::Result<usize>,
) -> NativeResult {
    match result {
        Ok(count) => {
            if wrapper == Wrapper::Output {
                context.write(&data[..count]);
            }
            Ok(Value::Int(count as i64))
        }
        Err(error) => {
            write_notice(context, function, wrapper, &error, data.len());
            Ok(Value::Bool(false))
        }
    }
}

pub fn write_bytes(stream: &mut Stream, data: &[u8]) -> io::Result<usize> {
    let mut position = 0;
    while position < data.len() {
        match stream.write(&data[position..]) {
            Ok(0) => break,
            Ok(written) => position += written,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if position == 0 => return Err(error),
            Err(_) => break,
        }
    }
    Ok(position)
}

fn fpassthru(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = stream_arg(&arguments, "fpassthru")?;
        let mut total = 0;
        loop {
            match read(stream, Some(crate::io_policy::CHUNK as u64)).await? {
                Ok(bytes) if bytes.is_empty() => break,
                Ok(bytes) => {
                    total += bytes.len();
                    context.write(&bytes);
                }
                Err(error) => {
                    read_notice(context, "fpassthru", &error, read_amount(None));
                    break;
                }
            }
        }
        Ok(Value::Int(total as i64))
    })
}
