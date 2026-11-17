//! PHP's byte-oriented formatted output and positional argument parser.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

mod parser;
mod value;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arguments {
    Variadic,
    Array,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Destination {
    Return,
    Output,
}

const VARIADIC_PARAMS: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("values", ParamType::Mixed).variadic(),
];

const ARRAY_PARAMS: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("values", ParamType::Array),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("sprintf", VARIADIC_PARAMS, sprintf));
    registry.function(FunctionEntry::sync("printf", VARIADIC_PARAMS, printf));
    registry.function(FunctionEntry::sync("vsprintf", ARRAY_PARAMS, vsprintf));
    registry.function(FunctionEntry::sync("vprintf", ARRAY_PARAMS, vprintf));
}

fn sprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        "sprintf",
        Arguments::Variadic,
        Destination::Return,
    )
}

fn printf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        "printf",
        Arguments::Variadic,
        Destination::Output,
    )
}

fn vsprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        "vsprintf",
        Arguments::Array,
        Destination::Return,
    )
}

fn vprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        "vprintf",
        Arguments::Array,
        Destination::Output,
    )
}

/// The vector forms consume values in insertion order, ignoring array keys.
fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    vector: Arguments,
    output: Destination,
) -> NativeResult {
    let values: Vec<Value> = if vector == Arguments::Array {
        args::array(arguments, 1)
            .map(|array| array.values().cloned().collect())
            .unwrap_or_default()
    } else {
        arguments.get(1..).unwrap_or_default().to_vec()
    };
    let result = render(context, args::bytes(arguments, 0), &values, name, vector)?;
    if output == Destination::Output {
        context.write(&result);
        Ok(Value::Int(result.len() as i64))
    } else {
        Ok(bytes(result))
    }
}

/// Array forms report value errors; variadic forms include the format argument.
fn missing(_name: &str, vector: Arguments, needed: usize, given: usize) -> NativeError {
    if vector == Arguments::Array {
        NativeError::value_error(format!(
            "The arguments array must contain {needed} items, {given} given"
        ))
    } else {
        NativeError::argument_count_error(format!(
            "{needed} arguments are required, {given} given",
            needed = needed + 1,
            given = given + 1
        ))
    }
}

/// Rendering is buffered so printf emits nothing if a later specifier fails.
fn render(
    context: &mut dyn Context,
    format: &[u8],
    values: &[Value],
    name: &str,
    vector: Arguments,
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
        let Some(text) = specifier.convert(context, value, name, &mut result)? else {
            continue;
        };
        specifier.append(&mut result, text)?;
    }
    if parser.needed > 0 {
        return Err(missing(name, vector, parser.needed, values.len()));
    }
    Ok(result)
}
