//! Numbers in other bases: `dechex`, `hexdec`, `base_convert` and their
//! relatives, and `fpow`.

use phera_extension_api::{
    Change, Context, FunctionEntry, LanguageLevel, NativeError, NativeFn, NativeResult, Param,
    ParamType, Value, args,
};

pub(super) fn functions(level: LanguageLevel) -> [FunctionEntry; 8] {
    let notice = level.has(Change::BaseConversionOverflowNotice);
    let select = |entry: FunctionEntry, updated| {
        let handler = if notice {
            NativeFn::Sync(updated)
        } else {
            entry.handler
        };
        FunctionEntry { handler, ..entry }
    };
    [
        DECHEX,
        DECBIN,
        DECOCT,
        select(HEXDEC, hexdec::<true>),
        select(BINDEC, bindec::<true>),
        select(OCTDEC, octdec::<true>),
        select(BASE_CONVERT, base_convert::<true>),
        FPOW,
    ]
}

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// The size of the buffer PHP converts a float into, which caps the
/// digits. No double needs that many: 2^1024 has 1025 binary digits.
const ZEND_DOUBLE_MAX_LENGTH: usize = 1080;

/// `dechex(int $num): string`
///
/// ```php
/// dechex(255); // "ff"
/// dechex(-1);  // "ffffffffffffffff": a negative int is read as unsigned
/// ```
const DECHEX: FunctionEntry =
    FunctionEntry::sync("dechex", &[Param::new("num", ParamType::Int)], dechex);

fn dechex(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default();
    Ok(Value::from(integer_digits(number as u64, 16)))
}

/// `decbin(int $num): string`
///
/// ```php
/// decbin(5); // "101"
/// ```
const DECBIN: FunctionEntry =
    FunctionEntry::sync("decbin", &[Param::new("num", ParamType::Int)], decbin);

fn decbin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default();
    Ok(Value::from(integer_digits(number as u64, 2)))
}

/// `decoct(int $num): string`
///
/// ```php
/// decoct(8); // "10"
/// ```
const DECOCT: FunctionEntry =
    FunctionEntry::sync("decoct", &[Param::new("num", ParamType::Int)], decoct);

fn decoct(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default();
    Ok(Value::from(integer_digits(number as u64, 8)))
}

/// `hexdec(string $hex_string): int|float`, a float once the number passes
/// `PHP_INT_MAX`.
///
/// ```php
/// hexdec("ff");  // 255
/// hexdec("0xff"); // 255
/// hexdec("fg");  // 15, with a deprecation for the "g"
/// ```
const HEXDEC: FunctionEntry = FunctionEntry::sync(
    "hexdec",
    &[Param::new("hex_string", ParamType::String)],
    hexdec::<false>,
);

fn hexdec<const OVERFLOW_NOTICE: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    Ok(parse::<OVERFLOW_NOTICE>(
        context,
        args::bytes(arguments, 0),
        16,
    ))
}

/// `bindec(string $binary_string): int|float`, a float once the number passes
/// `PHP_INT_MAX`.
///
/// ```php
/// bindec("101"); // 5
/// ```
const BINDEC: FunctionEntry = FunctionEntry::sync(
    "bindec",
    &[Param::new("binary_string", ParamType::String)],
    bindec::<false>,
);

fn bindec<const OVERFLOW_NOTICE: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    Ok(parse::<OVERFLOW_NOTICE>(
        context,
        args::bytes(arguments, 0),
        2,
    ))
}

/// `octdec(string $octal_string): int|float`, a float once the number passes
/// `PHP_INT_MAX`.
///
/// ```php
/// octdec("777"); // 511
/// ```
const OCTDEC: FunctionEntry = FunctionEntry::sync(
    "octdec",
    &[Param::new("octal_string", ParamType::String)],
    octdec::<false>,
);

fn octdec<const OVERFLOW_NOTICE: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    Ok(parse::<OVERFLOW_NOTICE>(
        context,
        args::bytes(arguments, 0),
        8,
    ))
}

/// `base_convert(string $num, int $from_base, int $to_base): string`
///
/// Past `PHP_INT_MAX` PHP goes on in floats, and so loses digits.
///
/// ```php
/// base_convert("ff", 16, 2); // "11111111"
/// ```
const BASE_CONVERT: FunctionEntry = FunctionEntry::sync(
    "base_convert",
    &[
        Param::new("num", ParamType::String),
        Param::new("from_base", ParamType::Int),
        Param::new("to_base", ParamType::Int),
    ],
    base_convert::<false>,
);

/// `fpow(float $num, float $exponent): float`, new in PHP 8.4: IEEE power,
/// where `pow()` may return an int.
///
/// ```php
/// fpow(2, 3); // 8.0
/// ```
const FPOW: FunctionEntry = FunctionEntry::sync(
    "fpow",
    &[
        Param::new("num", ParamType::Float),
        Param::new("exponent", ParamType::Float),
    ],
    fpow,
);

fn fpow(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let base = args::float(arguments, 0).unwrap_or_default();
    let exponent = args::float(arguments, 1).unwrap_or_default();
    Ok(Value::Float(base.powf(exponent)))
}

/// Signed inputs are reinterpreted as unsigned for the dec* functions.
fn integer_digits(mut number: u64, base: u64) -> String {
    let mut result = Vec::new();
    loop {
        result.push(DIGITS[(number % base) as usize]);
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
fn parse<const OVERFLOW_NOTICE: bool>(
    context: &mut dyn Context,
    string: &[u8],
    base: i64,
) -> Value {
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
    if OVERFLOW_NOTICE && matches!(number, ParsedNumber::Floating(_)) {
        context.diagnostics().notice(
            "Input number is larger than PHP_INT_MAX, precision has been lost in conversion".into(),
        );
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
fn base_convert<const OVERFLOW_NOTICE: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    let from = args::int(arguments, 1).unwrap_or_default();
    let to = args::int(arguments, 2).unwrap_or_default();
    for (base, index, name) in [(from, 2, "from_base"), (to, 3, "to_base")] {
        if !(2..=36).contains(&base) {
            return Err(NativeError::value_error(format!(
                "base_convert(): Argument #{index} (${name}) must be between 2 and 36 (inclusive)"
            )));
        }
    }
    match parse::<OVERFLOW_NOTICE>(context, args::bytes(arguments, 0), from) {
        Value::Int(number) => Ok(Value::from(integer_digits(number as u64, to as u64))),
        Value::Float(mut number) => {
            if !number.is_finite() {
                return Err(NativeError::value_error(format!(
                    "An infinite value cannot be converted to base {to}"
                )));
            }
            let mut result = Vec::new();
            loop {
                result.push(DIGITS[(number % to as f64) as usize]);
                number /= to as f64;
                if number < 1.0 || result.len() == ZEND_DOUBLE_MAX_LENGTH {
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
        string = &string[1..];
    }
    while string
        .last()
        .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == 11)
    {
        string = &string[..string.len() - 1];
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
        string = &string[2..];
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
