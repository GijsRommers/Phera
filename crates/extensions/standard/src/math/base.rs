//! Radix conversion retains integer precision until PHP's signed limit is exceeded.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

const INTEGER_PARAMS: &[Param] = &[Param::new("num", ParamType::Int)];

const STRING_PARAMS: &[Param] = &[Param::new("string", ParamType::String)];

const CONVERT_PARAMS: &[Param] = &[
    Param::new("num", ParamType::String),
    Param::new("from_base", ParamType::Int),
    Param::new("to_base", ParamType::Int),
];

const POWER_PARAMS: &[Param] = &[
    Param::new("num", ParamType::Float),
    Param::new("exponent", ParamType::Float),
];

const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("dechex", INTEGER_PARAMS, dechex));
    registry.function(FunctionEntry::sync("decbin", INTEGER_PARAMS, decbin));
    registry.function(FunctionEntry::sync("decoct", INTEGER_PARAMS, decoct));
    registry.function(FunctionEntry::sync("hexdec", STRING_PARAMS, hexdec));
    registry.function(FunctionEntry::sync("bindec", STRING_PARAMS, bindec));
    registry.function(FunctionEntry::sync("octdec", STRING_PARAMS, octdec));
    registry.function(FunctionEntry::sync(
        "base_convert",
        CONVERT_PARAMS,
        base_convert,
    ));
    registry.function(FunctionEntry::sync("fpow", POWER_PARAMS, fpow));
}

fn dechex(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(integer_digits(
        args::int(arguments, 0).unwrap_or_default() as u64,
        16,
    )))
}

fn decbin(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(integer_digits(
        args::int(arguments, 0).unwrap_or_default() as u64,
        2,
    )))
}

fn decoct(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(integer_digits(
        args::int(arguments, 0).unwrap_or_default() as u64,
        8,
    )))
}

fn hexdec(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(parse(context, args::bytes(arguments, 0), 16))
}

fn bindec(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(parse(context, args::bytes(arguments, 0), 2))
}

fn octdec(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(parse(context, args::bytes(arguments, 0), 8))
}

fn fpow(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0)
            .unwrap_or_default()
            .powf(args::float(arguments, 1).unwrap_or_default()),
    ))
}

/// Signed inputs are reinterpreted as unsigned for the dec* functions.
fn integer_digits(mut number: u64, base: u64) -> String {
    let mut result = Vec::new();
    loop {
        if let Some(digit) = DIGITS.get((number % base) as usize) {
            result.push(*digit);
        }
        number /= base;
        if number == 0 {
            break;
        }
    }
    result.reverse();
    result.into_iter().map(char::from).collect()
}

/// Invalid characters are skipped but cause one deprecation per conversion.
/// Overflow intentionally switches to floating arithmetic, matching the runtime.
fn parse(context: &mut dyn Context, string: &[u8], base: i64) -> Value {
    let string = strip_prefix(string, base);
    let mut number = ParsedNumber::Integer(0);
    let mut invalid = false;
    for &byte in string {
        let Some(digit) = char::from(byte).to_digit(base as u32) else {
            invalid = true;
            continue;
        };
        number = number.append(i64::from(digit), base);
    }
    if invalid {
        context.deprecated(
            "Invalid characters passed for attempted conversion, these have been ignored".into(),
        );
    }
    match number {
        ParsedNumber::Integer(integer) => Value::Int(integer),
        ParsedNumber::Floating(number) => Value::Float(number),
    }
}

/// Overflow uses floating radix division, intentionally retaining PHP precision loss.
fn base_convert(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let from = args::int(arguments, 1).unwrap_or_default();
    let to = args::int(arguments, 2).unwrap_or_default();
    for (base, index, name) in [(from, 2, "from_base"), (to, 3, "to_base")] {
        if !(2..=36).contains(&base) {
            return Err(NativeError::value_error(format!(
                "base_convert(): Argument #{index} (${name}) must be between 2 and 36 (inclusive)"
            )));
        }
    }
    match parse(context, args::bytes(arguments, 0), from) {
        Value::Int(number) => Ok(Value::from(integer_digits(number as u64, to as u64))),
        Value::Float(mut number) => {
            if !number.is_finite() {
                return Err(NativeError::value_error(format!(
                    "An infinite value cannot be converted to base {to}"
                )));
            }
            let mut result = Vec::new();
            loop {
                if let Some(digit) = DIGITS.get((number % to as f64) as usize) {
                    result.push(*digit);
                }
                number /= to as f64;
                if number < 1.0 || result.len() == 1080 {
                    break;
                }
            }
            result.reverse();
            Ok(Value::from(
                result.into_iter().map(char::from).collect::<String>(),
            ))
        }
        _ => Ok(Value::from("0")),
    }
}

/// Whitespace and conventional radix prefixes are ignored before digit conversion.
fn strip_prefix(string: &[u8], base: i64) -> &[u8] {
    let mut string = string;
    while string
        .first()
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == 11)
    {
        string = string.get(1..).unwrap_or_default();
    }
    while string
        .last()
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == 11)
    {
        string = string.get(..string.len() - 1).unwrap_or_default();
    }
    let prefix = match base {
        16 => b'x',
        8 => b'o',
        2 => b'b',
        _ => 0,
    };
    if string.first() == Some(&b'0')
        && string
            .get(1)
            .is_some_and(|byte| byte.to_ascii_lowercase() == prefix)
    {
        string = string.get(2..).unwrap_or_default();
    }
    string
}

enum ParsedNumber {
    Integer(i64),
    Floating(f64),
}

impl ParsedNumber {
    /// Once signed integer arithmetic overflows, PHP stays in floating arithmetic.
    fn append(self, digit: i64, base: i64) -> Self {
        match self {
            Self::Integer(integer) => integer
                .checked_mul(base)
                .and_then(|number| number.checked_add(digit))
                .map(Self::Integer)
                .unwrap_or_else(|| Self::Floating(integer as f64 * base as f64 + digit as f64)),
            Self::Floating(number) => Self::Floating(number * base as f64 + digit as f64),
        }
    }
}
