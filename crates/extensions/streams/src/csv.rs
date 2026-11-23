//! `fputcsv($file, ['a,b', 'c'], escape: '')` writes standard CSV quoting.

use phera_ext_standard::csv::{Dialect, ENCLOSURE, ESCAPE, SEPARATOR, values};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};

use crate::handles::{LENGTH, STREAM, read_amount, read_notice};
use crate::resource::{operate, stream_arg, with_stream};
use crate::transfer::write;

pub fn register(registry: &mut Registry) {
    const READ: &[Param] = &[STREAM, LENGTH, SEPARATOR, ENCLOSURE, ESCAPE];
    const WRITE: &[Param] = &[
        STREAM,
        Param::new("fields", ParamType::Array),
        SEPARATOR,
        ENCLOSURE,
        ESCAPE,
        Param::new("eol", ParamType::String).with_default(ConstantValue::String("\n")),
    ];
    registry.function(FunctionEntry::asynchronous("fgetcsv", READ, get));
    registry.function(FunctionEntry::asynchronous("fputcsv", WRITE, put));
}

fn put(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "fputcsv";
        let stream = stream_arg(&args, name)?;
        let dialect = Dialect::from_arguments(context, name, &args, 2)?;
        let mut line = Vec::new();
        if let Some(fields) = args::array(&args, 1) {
            for (index, value) in fields.values().enumerate() {
                if index > 0 {
                    line.push(dialect.separator());
                }
                let field = context.stringify(value).await?;
                dialect.encode(field.as_bytes(), &mut line);
            }
        }
        line.extend_from_slice(args::string(&args, 5).map_or(b"\n", |string| string.as_bytes()));
        write(context, stream, &line, name).await
    })
}

fn get(context: &mut dyn Context, args: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "fgetcsv";
        let stream = stream_arg(&args, name)?;
        let dialect = Dialect::from_arguments(context, name, &args, 2)?;
        let length = args::int(&args, 1).unwrap_or(0);
        if !(0..i64::MAX).contains(&length) {
            return Err(NativeError::value_error(format!(
                "{name}(): Argument #2 ($length) must be between 0 and {}",
                i64::MAX - 1
            )));
        }
        let length = (length > 0).then_some(length as u64);
        let policy = with_stream(stream, |stream| stream.read_policy(None))?;
        let fields = match operate(stream, policy, move |stream| {
            let line = stream.line(length)?;
            if line.is_empty() {
                return Ok(None);
            }
            dialect.decode(line, || stream.line(None)).map(Some)
        })
        .await?
        {
            Ok(Some(fields)) => fields,
            Ok(None) => return Ok(Value::Bool(false)),
            Err(error) => {
                read_notice(context, name, &error, read_amount(length));
                return Ok(Value::Bool(false));
            }
        };
        values(fields)
    })
}
