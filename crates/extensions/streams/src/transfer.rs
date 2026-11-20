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
    Box::pin(fread_arguments(context, arguments, "fread"))
}

pub(crate) async fn fread_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let resource = stream_arg(&arguments, function)?;
    let length = positive(function, args::int(&arguments, 1).unwrap_or(0))?;
    match read_context(context, resource, function, Some(length), false).await? {
        Ok(bytes) => Ok(Value::from(bytes)),
        Err(error) => {
            read_notice(context, function, &error, read_amount(Some(length)));
            Ok(Value::Bool(false))
        }
    }
}

fn fgetc(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(fgetc_arguments(context, arguments, "fgetc"))
}

pub(crate) async fn fgetc_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let resource = stream_arg(&arguments, function)?;
    match read_context(context, resource, function, Some(1), false).await? {
        Ok(bytes) if !bytes.is_empty() => Ok(Value::from(bytes)),
        Ok(_) => Ok(Value::Bool(false)),
        Err(error) => {
            read_notice(context, function, &error, read_amount(Some(1)));
            Ok(Value::Bool(false))
        }
    }
}

fn fgets(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(fgets_arguments(context, arguments, "fgets"))
}

pub(crate) async fn fgets_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let resource = stream_arg(&arguments, function)?;
    let length = args::int(&arguments, 1)
        .map(|length| positive(function, length).map(|length| length - 1))
        .transpose()?;
    match read_context(context, resource, function, length, true).await? {
        Ok(bytes) if !bytes.is_empty() => Ok(Value::from(bytes)),
        Ok(_) => Ok(Value::Bool(false)),
        Err(error) => {
            read_notice(context, function, &error, read_amount(None));
            Ok(Value::Bool(false))
        }
    }
}

pub async fn read_context(
    context: &mut dyn Context,
    resource: &Resource,
    function: &str,
    length: Option<u64>,
    by_line: bool,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    if let Some(chains) = crate::filters::user::lookup(context, resource.id)
        && chains.reading()
    {
        let size = length
            .and_then(|length| usize::try_from(length).ok())
            .unwrap_or(usize::MAX);
        return chains
            .read(context, resource, function, size, by_line)
            .await;
    }
    read_unfiltered(context, resource, function, length, by_line).await
}

pub async fn read_unfiltered(
    context: &mut dyn Context,
    resource: &Resource,
    function: &str,
    length: Option<u64>,
    by_line: bool,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    if let Some(file) = crate::user_file::resource_file(resource) {
        let length = length
            .and_then(|size| usize::try_from(size).ok())
            .unwrap_or(usize::MAX);
        return file.read(context, function, length, by_line).await;
    }
    let action = with_stream(resource, |stream| {
        if let Some(pipe) = stream.child_pipe.clone() {
            ReadAction::Pipe(pipe)
        } else if matches!(
            stream.read_policy(length),
            crate::io_policy::IoPolicy::Inline
        ) {
            ReadAction::Done(read_native(stream, length, by_line))
        } else {
            ReadAction::Blocking
        }
    })?;
    match action {
        ReadAction::Pipe(pipe) => {
            if by_line {
                crate::child_pipe::wait_with_execution_limit(context, pipe.line(resource, length))
                    .await?
            } else {
                crate::child_pipe::wait_with_execution_limit(context, pipe.read(resource, length))
                    .await?
            }
        }
        ReadAction::Blocking => {
            operate(
                resource,
                crate::io_policy::IoPolicy::Blocking,
                move |stream| read_native(stream, length, by_line),
            )
            .await
        }
        ReadAction::Done(result) => Ok(result),
    }
}

enum ReadAction {
    Pipe(crate::child_pipe::ChildPipe),
    Blocking,
    Done(io::Result<Vec<u8>>),
}

fn read_native(stream: &mut Stream, length: Option<u64>, by_line: bool) -> io::Result<Vec<u8>> {
    if by_line {
        stream.line(length)
    } else {
        stream.read(length)
    }
}

pub async fn line(
    resource: &Resource,
    length: Option<u64>,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    if let Some(pipe) = crate::child_pipe::of(resource)? {
        return pipe.line(resource, length).await;
    }
    let policy = with_stream(resource, |stream| stream.read_policy(length))?;
    operate(resource, policy, move |stream| stream.line(length)).await
}

pub async fn read(
    resource: &Resource,
    length: Option<u64>,
) -> Result<io::Result<Vec<u8>>, phera_extension_api::NativeError> {
    if let Some(pipe) = crate::child_pipe::of(resource)? {
        return pipe.read(resource, length).await;
    }
    let policy = with_stream(resource, |stream| stream.read_policy(length))?;
    operate(resource, policy, move |stream| stream.read(length)).await
}

fn fwrite(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(write_arguments(context, arguments, "fwrite"))
}

fn fputs(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(write_arguments(context, arguments, "fputs"))
}

pub(crate) async fn write_arguments(
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
    if let Some(chains) = crate::filters::user::lookup(context, resource.id)
        && chains.writing()
    {
        return chains.write(context, resource, function, data).await;
    }
    write_unfiltered(context, resource, data, function).await
}

pub async fn write_unfiltered(
    context: &mut dyn Context,
    resource: &Resource,
    data: &[u8],
    function: &str,
) -> NativeResult {
    if let Some(file) = crate::user_file::resource_file(resource) {
        return Ok(file
            .write(context, function, data)
            .await?
            .map_or(Value::Bool(false), |count| Value::Int(count as i64)));
    }
    let length = data.len();
    let (wrapper, action) = with_stream(resource, |stream| {
        let wrapper = stream.wrapper;
        let action = if let Some(pipe) = stream.child_pipe.clone() {
            WriteAction::Pipe(pipe)
        } else if wrapper == Wrapper::Output
            && stream
                .filters
                .as_ref()
                .is_some_and(|filters| filters.write.active())
        {
            WriteAction::Output(stream.filter_write(data, crate::filters::Flush::No))
        } else if wrapper != Wrapper::Output
            && matches!(
                stream.write_policy(length as u64),
                crate::io_policy::IoPolicy::Blocking
            )
        {
            WriteAction::Blocking
        } else {
            WriteAction::Done(stream.write(data))
        };
        (wrapper, action)
    })?;
    let result = match action {
        WriteAction::Pipe(pipe) => {
            crate::child_pipe::wait_with_execution_limit(context, pipe.write(resource, data))
                .await??
        }
        WriteAction::Blocking => {
            let data = data.to_vec();
            operate(
                resource,
                crate::io_policy::IoPolicy::Blocking,
                move |stream| write_bytes(stream, &data),
            )
            .await?
        }
        WriteAction::Output(output) => {
            return match output {
                Ok(output) => {
                    context.write(&output);
                    Ok(Value::Int(length as i64))
                }
                Err(error) => {
                    context.warning(format!("{function}(): {error}"));
                    Ok(Value::Bool(false))
                }
            };
        }
        WriteAction::Done(result) => result,
    };
    finish_write(context, wrapper, data, function, result)
}

enum WriteAction {
    Pipe(crate::child_pipe::ChildPipe),
    Blocking,
    Output(io::Result<Vec<u8>>),
    Done(io::Result<usize>),
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
    Box::pin(fpassthru_arguments(context, arguments, "fpassthru"))
}

pub(crate) async fn fpassthru_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let stream = stream_arg(&arguments, function)?;
    let mut total = 0;
    loop {
        let result = read_context(
            context,
            stream,
            function,
            Some(crate::io_policy::CHUNK as u64),
            false,
        )
        .await?;
        match result {
            Ok(bytes) if bytes.is_empty() => break,
            Ok(bytes) => {
                total += bytes.len();
                if let Some(flush) = context.write_chunk(&bytes) {
                    flush.await?;
                }
            }
            Err(error) => {
                read_notice(context, function, &error, read_amount(None));
                return Ok(Value::Int(if total == 0 { -1 } else { total as i64 }));
            }
        }
    }
    Ok(Value::Int(total as i64))
}
