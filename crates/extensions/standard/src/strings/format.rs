//! `sprintf("%05.1f|%-4s|%x", 3.14159, "ab", 255)` is `"003.1|ab  |ff"`.
//!
//! [`parser`] reads one `%` specifier with its flags, width and precision;
//! [`value`] converts the argument and pads it.

use std::borrow::Cow;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args, write_after_diagnostics,
};

mod parser;
mod render;
mod value;

use self::render::Formatter;
pub use self::render::render;

pub(super) fn register(registry: &mut Registry) {
    registry.register_sync_selector("sprintf", |slots| {
        scalar_arguments(slots).then_some(|context, arguments| {
            format_sync(
                context,
                arguments,
                "sprintf",
                Values::Arguments,
                Destination::Return,
            )
        })
    });
    registry.register_sync_selector("printf", |slots| {
        (quiet_format(slots)
            && slots
                .iter()
                .skip(1)
                .all(|slot| slot.as_ref().is_some_and(quiet_scalar)))
        .then_some(|context, arguments| {
            format_sync(
                context,
                arguments,
                "printf",
                Values::Arguments,
                Destination::Output,
            )
        })
    });
    registry.register_sync_selector("vsprintf", |slots| {
        scalar_array(slots).then_some(|context, arguments| {
            format_sync(
                context,
                arguments,
                "vsprintf",
                Values::Array,
                Destination::Return,
            )
        })
    });
    registry.register_sync_selector("vprintf", |slots| {
        (quiet_format(slots)
            && matches!(slots, [_, Some(Value::Array(array))] if array.values().all(quiet_scalar)))
        .then_some(|context, arguments| {
            format_sync(
                context,
                arguments,
                "vprintf",
                Values::Array,
                Destination::Output,
            )
        })
    });
    for entry in [SPRINTF, PRINTF, VSPRINTF, VPRINTF] {
        registry.function_with_native_frame(entry);
    }
}

fn scalar(value: &Value) -> bool {
    !matches!(
        value,
        Value::Object(_) | Value::Reference(_) | Value::Array(_)
    )
}

fn scalar_arguments(slots: &[Option<Value>]) -> bool {
    slots
        .iter()
        .skip(1)
        .all(|slot| slot.as_ref().is_some_and(scalar))
}

fn scalar_array(slots: &[Option<Value>]) -> bool {
    matches!(slots, [_, Some(Value::Array(array))] if array.values().all(scalar))
}

// An explicit precision can produce a notice. Floats outside the integer
// range and NaN can warn during a cast. Send both shapes to the output path
// that can await a user error handler.
fn quiet_format(slots: &[Option<Value>]) -> bool {
    slots
        .first()
        .and_then(Option::as_ref)
        .and_then(Value::as_string)
        .map(|string| string.as_bytes())
        .is_some_and(|format| !format.contains(&b'.'))
}

fn quiet_scalar(value: &Value) -> bool {
    scalar(value) && !matches!(value, Value::Float(_))
}

/// Where the values come from: the arguments, or one array of them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Values {
    Arguments,
    StreamArguments,
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

fn format_values(arguments: &[Value], source: Values) -> Cow<'_, [Value]> {
    match source {
        Values::Arguments | Values::StreamArguments => Cow::Borrowed(&arguments[1..]),
        Values::Array => Cow::Owned(
            crate::arguments::required_array(arguments, 1)
                .values()
                .cloned()
                .collect(),
        ),
    }
}

fn format_sync(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    source: Values,
    destination: Destination,
) -> NativeResult {
    let values = format_values(arguments, source);
    let mut formatter = Formatter::new(args::bytes(arguments, 0), &values)?;
    if formatter.advance(context, function, source)?.is_some() {
        return Err(NativeError::error(
            "Object conversion requires the native call frame",
        ));
    }
    Ok(destination.finish(context, formatter.result))
}

async fn format(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    source: Values,
    destination: Destination,
) -> NativeResult {
    let values = format_values(&arguments, source);
    let result = render(
        context,
        args::bytes(&arguments, 0),
        &values,
        function,
        source,
    )
    .await?;
    match destination {
        Destination::Return => Ok(Value::from(result)),
        Destination::Output => {
            write_after_diagnostics(context, &result).await?;
            Ok(Value::Int(result.len() as i64))
        }
    }
}

impl Destination {
    fn finish(self, context: &mut dyn Context, result: Vec<u8>) -> Value {
        match self {
            Self::Return => Value::from(result),
            Self::Output => {
                context.write(&result);
                Value::Int(result.len() as i64)
            }
        }
    }
}
