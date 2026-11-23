//! `fprintf(STDERR, 'Invalid id: %d', $id)` formats bytes and writes them to STDERR.

use std::borrow::Cow;

use phera_ext_standard::{FormatValues, render_format};
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::handles::STREAM;
use crate::resource::stream_arg;
use crate::transfer::write;

pub fn register(registry: &mut Registry) {
    const FORMAT: Param = Param::new("format", ParamType::String);
    const LIST: &[Param] = &[
        STREAM,
        FORMAT,
        Param::new("values", ParamType::Mixed).variadic(),
    ];
    const ARRAY: &[Param] = &[STREAM, FORMAT, Param::new("values", ParamType::Array)];
    registry.function(FunctionEntry::asynchronous("fprintf", LIST, fprintf));
    registry.function(FunctionEntry::asynchronous("vfprintf", ARRAY, vfprintf));
}

fn fprintf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format_write(
        context,
        arguments,
        "fprintf",
        FormatValues::StreamArguments,
    ))
}

fn vfprintf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format_write(
        context,
        arguments,
        "vfprintf",
        FormatValues::Array,
    ))
}

async fn format_write(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    source: FormatValues,
) -> NativeResult {
    let stream = stream_arg(&arguments, function)?;
    let format = args::bytes(&arguments, 1);
    let values: Cow<[Value]> = match source {
        FormatValues::Array => Cow::Owned(
            args::array(&arguments, 2)
                .map_or_else(Vec::new, |values| values.values().cloned().collect()),
        ),
        _ => Cow::Borrowed(arguments.get(2..).unwrap_or_default()),
    };
    let data = render_format(context, format, &values, function, source).await?;
    let requested = data.len() as i64;
    let unwind = phera_extension_api::before_output(context).await?;
    let written = write(context, stream, &data, function).await;
    if let Some(error) = unwind {
        return Err(error);
    }
    written?;
    Ok(Value::Int(requested))
}
