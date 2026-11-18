//! `sprintf("%05.1f|%-4s|%x", 3.14159, "ab", 255)` is `"003.1|ab  |ff"`.
//!
//! [`parser`] reads one `%` specifier with its flags, width and precision;
//! [`value`] converts the argument and pads it.

use std::borrow::Cow;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::buffer;

mod parser;
mod value;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[SPRINTF, PRINTF, VSPRINTF, VPRINTF];

/// Where the values come from: the arguments, or one array of them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Values {
    Arguments,
    Array,
}

/// What happens to the formatted string.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Return,
    Output,
}

/// `sprintf(string $format, mixed ...$values): string`
///
/// ```php
/// sprintf("%s has %d items", "cart", 3);   // "cart has 3 items"
/// sprintf("%05.1f", 3.14159);              // "003.1"
/// sprintf('%2$s %1$s', "world", "hello");  // "hello world"
/// sprintf("%d %d", 1);                     // ArgumentCountError
/// ```
const SPRINTF: FunctionEntry = FunctionEntry::asynchronous(
    "sprintf",
    &[
        Param::new("format", ParamType::String),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    sprintf,
);

fn sprintf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format(
        context,
        arguments,
        "sprintf",
        Values::Arguments,
        Destination::Return,
    ))
}

/// `printf(string $format, mixed ...$values): int`, which writes the
/// result and returns its length.
const PRINTF: FunctionEntry = FunctionEntry::asynchronous(
    "printf",
    &[
        Param::new("format", ParamType::String),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    printf,
);

fn printf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format(
        context,
        arguments,
        "printf",
        Values::Arguments,
        Destination::Output,
    ))
}

/// `vsprintf(string $format, array $values): string`, taking the values in
/// array order; the keys do not matter.
///
/// ```php
/// vsprintf("%s-%s", ["b" => 1, "a" => 2]); // "1-2"
/// ```
const VSPRINTF: FunctionEntry = FunctionEntry::asynchronous(
    "vsprintf",
    &[
        Param::new("format", ParamType::String),
        Param::new("values", ParamType::Array),
    ],
    vsprintf,
);

fn vsprintf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format(
        context,
        arguments,
        "vsprintf",
        Values::Array,
        Destination::Return,
    ))
}

/// `vprintf(string $format, array $values): int`
const VPRINTF: FunctionEntry = FunctionEntry::asynchronous(
    "vprintf",
    &[
        Param::new("format", ParamType::String),
        Param::new("values", ParamType::Array),
    ],
    vprintf,
);

fn vprintf(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(format(
        context,
        arguments,
        "vprintf",
        Values::Array,
        Destination::Output,
    ))
}

async fn format(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    source: Values,
    destination: Destination,
) -> NativeResult {
    let arguments = &arguments;
    let values: Cow<[Value]> = match source {
        Values::Arguments => Cow::Borrowed(&arguments[1..]),
        Values::Array => Cow::Owned(
            crate::arguments::required_array(arguments, 1)
                .values()
                .cloned()
                .collect(),
        ),
    };
    let result = render(
        context,
        args::bytes(arguments, 0),
        &values,
        function,
        source,
    )
    .await?;
    match destination {
        Destination::Return => Ok(Value::from(result)),
        Destination::Output => {
            context.write(&result);
            Ok(Value::Int(result.len() as i64))
        }
    }
}

/// The whole result is built before anything is written, so `printf`
/// writes nothing when a later specifier fails.
async fn render(
    context: &mut dyn Context,
    format: &[u8],
    values: &[Value],
    function: &str,
    source: Values,
) -> Result<Vec<u8>, NativeError> {
    let mut result = buffer(format.len())?;
    let mut parser = parser::Parser::new(format, values);
    while let Some(&byte) = format.get(parser.cursor) {
        parser.cursor += 1;
        if byte != b'%' {
            result.push(byte);
            continue;
        }
        if format.get(parser.cursor) == Some(&b'%') {
            result.push(b'%');
            parser.cursor += 1;
            continue;
        }
        let Some((specifier, value)) = parser.specifier()? else {
            continue;
        };
        let Some(text) = specifier
            .convert(context, value, function, &mut result)
            .await?
        else {
            continue;
        };
        specifier.append(&mut result, text)?;
    }
    if parser.needed > 0 {
        return Err(too_few_values(source, parser.needed, values.len()));
    }
    Ok(result)
}

/// PHP's error when the format refers to more values than there are. The
/// argument forms count the format as an argument too.
fn too_few_values(source: Values, needed: usize, given: usize) -> NativeError {
    match source {
        Values::Array => NativeError::value_error(format!(
            "The arguments array must contain {needed} items, {given} given"
        )),
        Values::Arguments => NativeError::argument_count_error(format!(
            "{} arguments are required, {} given",
            needed + 1,
            given + 1
        )),
    }
}
