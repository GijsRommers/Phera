//! Converting between encodings, checking them and guessing them:
//!
//! ```php
//! mb_convert_encoding('é', 'ISO-8859-1', 'UTF-8');   // "\xE9"
//! mb_convert_encoding(['k' => 'é'], 'UTF-16BE');     // arrays are converted value by value
//! mb_check_encoding("\xFF", 'UTF-8');                // false
//! mb_detect_encoding("\xE9t\xE9", 'UTF-8,ISO-8859-1'); // "ISO-8859-1"
//! mb_detect_order();                                 // ["ASCII", "UTF-8"]
//! ```

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::codec::{self, BAD, Substitute, decode};
use crate::encoding::{self, Encoding, Transfer};
use crate::state::Settings;
use crate::text::encode_result;
use crate::walk;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_convert_encoding",
        MB_CONVERT_ENCODING,
        mb_convert_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_check_encoding",
        MB_CHECK_ENCODING,
        mb_check_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_detect_encoding",
        MB_DETECT_ENCODING,
        mb_detect_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "mb_detect_order",
        MB_DETECT_ORDER,
        mb_detect_order,
    ));
    registry.function(FunctionEntry::sync(
        "mb_convert_variables",
        MB_CONVERT_VARIABLES,
        mb_convert_variables,
    ));
}

const MB_CONVERT_ENCODING: &[Param] = &[
    Param::new("string", ParamType::ArrayOrString),
    Param::new("to_encoding", ParamType::String),
    Param::new("from_encoding", ParamType::ArrayOrString)
        .nullable()
        .optional(),
];

/// `mb_convert_encoding(array|string $string, string $to_encoding,
/// array|string|null $from_encoding = null): array|string|false`: with
/// several source encodings, the likeliest is guessed for each string.
fn mb_convert_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let to = encoding::named(
        ctx,
        &String::from_utf8_lossy(args::bytes(a, 1)),
        "mb_convert_encoding",
        2,
        "to_encoding",
    )?;
    let from = if args::value(a, 2).is_null() {
        vec![ctx.state().get_or_default::<Settings>().internal]
    } else {
        encoding_list(
            ctx,
            args::value(a, 2),
            "mb_convert_encoding",
            3,
            "from_encoding",
        )?
    };
    let subject = args::value(a, 0);
    // Base64 and Quoted-Printable encode the bytes, whatever their encoding.
    if matches!(
        to.transfer(),
        Some(Transfer::Base64 | Transfer::QuotedPrintable)
    ) && let Value::String(string) = subject
    {
        let bytes: Vec<u32> = string
            .as_bytes()
            .iter()
            .map(|&byte| u32::from(byte))
            .collect();
        return Ok(Value::from(codec::encode(&bytes, to, &Substitute::None)));
    }
    // Decoding Base64, uuencode or Quoted-Printable gives bytes, not text.
    if let [from] = from[..]
        && matches!(
            from.transfer(),
            Some(Transfer::Base64 | Transfer::Uuencode | Transfer::QuotedPrintable)
        )
    {
        let bytes = decode(args::bytes(a, 0), from);
        return Ok(Value::from(encode_result(ctx, &bytes, Encoding::EightBit)));
    }
    convert_value(ctx, subject.clone(), to, &from)
}

/// What remains to do while converting a value, kept on a stack so that
/// nested arrays need no recursion.
enum Work {
    Convert(Value),
    /// Build an array from these keys and the last converted values.
    Collect(Vec<ArrayKey>),
}

/// Converts a string, or the strings inside an array at any depth; other
/// values stay as they are.
fn convert_value(
    ctx: &mut dyn Context,
    value: Value,
    to: Encoding,
    from: &[Encoding],
) -> NativeResult {
    let mut pending = vec![Work::Convert(value)];
    let mut converted = Vec::new();
    while let Some(work) = pending.pop() {
        match work {
            Work::Convert(Value::Array(array)) => {
                pending.push(Work::Collect(
                    array.iter().map(|(key, _)| key.clone()).collect(),
                ));
                // Reversed, so that they come off the stack in order.
                let values: Vec<Value> = array.iter().map(|(_, value)| value.clone()).collect();
                pending.extend(values.into_iter().rev().map(Work::Convert));
            }
            Work::Convert(Value::String(string)) => {
                let source = match from {
                    [only] => Some(*only),
                    _ => guess(string.as_bytes(), from, true),
                };
                let Some(source) = source else {
                    ctx.warning(
                        "mb_convert_encoding(): Unable to detect character encoding".into(),
                    );
                    return Ok(Value::Bool(false));
                };
                let points = decode(string.as_bytes(), source);
                converted.push(Value::from(encode_result(ctx, &points, to)));
            }
            Work::Convert(other) => converted.push(other),
            Work::Collect(keys) => {
                let values = converted.split_off(converted.len() - keys.len());
                let mut array = PhpArray::new();
                for (key, value) in keys.into_iter().zip(values) {
                    array.insert(key, value);
                }
                converted.push(Value::from(array));
            }
        }
    }
    Ok(converted.pop().unwrap_or(Value::Null))
}

const MB_CHECK_ENCODING: &[Param] = &[
    Param::new("value", ParamType::ArrayOrString)
        .nullable()
        .optional(),
    crate::ENCODING,
];

/// `mb_check_encoding(array|string|null $value = null, ?string $encoding = null): bool`:
/// for an array, every string inside it and every string key.
fn mb_check_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding::argument(ctx, a, 1, "mb_check_encoding")?;
    if args::value(a, 0).is_null() {
        ctx.deprecated(
            "mb_check_encoding(): Calling mb_check_encoding() without argument is deprecated"
                .into(),
        );
        return Ok(Value::Bool(true));
    }
    let Ok(strings) = walk::strings(args::value(a, 0).clone()) else {
        return Ok(Value::Bool(false));
    };
    let valid = |string: &phera_extension_api::PhpString| {
        !decode(string.as_bytes(), encoding).contains(&BAD)
    };
    Ok(Value::Bool(
        strings.values.iter().chain(&strings.keys).all(valid),
    ))
}

const MB_DETECT_ENCODING: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("encodings", ParamType::ArrayOrString)
        .nullable()
        .optional(),
    Param::new("strict", ParamType::Bool).optional(),
];

/// `mb_detect_encoding(string $string, array|string|null $encodings = null,
/// bool $strict = false): string|false`
fn mb_detect_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let candidates = encoding_list(ctx, args::value(a, 1), "mb_detect_encoding", 2, "encodings")?;
    let strict = args::bool(a, 2).unwrap_or(false);
    Ok(guess(args::bytes(a, 0), &candidates, strict)
        .map_or(Value::Bool(false), |encoding| Value::from(encoding.name())))
}

/// The bitmap of code points PHP considers rare in real text, one bit each,
/// from php-src's `rare_cp_bitvec.h`.
const RARE: &[u8] = include_bytes!("../data/rare.bin");

/// PHP's guess: the candidate under which the string looks least unusual.
/// Each character costs a penalty, invalid sequences most; candidates
/// later in the list cost up to 30 percent more. Strict mode rules out
/// candidates the string is invalid in.
pub fn guess(bytes: &[u8], candidates: &[Encoding], strict: bool) -> Option<Encoding> {
    let penalty = |point: u32| match point {
        BAD => 1000,
        0x10000.. => 40,
        0x21..=0x2F => 6,
        _ if RARE[point as usize / 8] & (1 << (point % 8)) != 0 => 30,
        _ => 1,
    };
    candidates
        .iter()
        .enumerate()
        .filter_map(|(index, &encoding)| {
            let points = decode(bytes, encoding);
            if strict && points.contains(&BAD) {
                return None;
            }
            let score: usize = points.into_iter().map(penalty).sum();
            let position = 1.0 + 0.3 * index as f64 / candidates.len() as f64;
            Some((encoding, (score as f64 * position) as usize))
        })
        .min_by_key(|&(_, score)| score)
        .map(|(encoding, _)| encoding)
}

const MB_DETECT_ORDER: &[Param] = &[Param::new("encoding", ParamType::ArrayOrString)
    .nullable()
    .optional()];

/// `mb_detect_order(array|string|null $encoding = null): array|bool`: the
/// candidates `mb_detect_encoding` and `"auto"` mean.
fn mb_detect_order(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    if args::value(a, 0).is_null() {
        let order = &ctx.state().get_or_default::<Settings>().detect;
        return Ok(crate::names::list(
            order.iter().map(|encoding| Value::from(encoding.name())),
        ));
    }
    let order = encoding_list(ctx, args::value(a, 0), "mb_detect_order", 1, "encoding")?;
    ctx.state().get_or_default::<Settings>().detect = order;
    Ok(Value::Bool(true))
}

const MB_CONVERT_VARIABLES: &[Param] = &[
    Param::new("to_encoding", ParamType::String),
    Param::new("from_encoding", ParamType::ArrayOrString),
    Param::new("var", ParamType::Mixed).nullable().by_ref(),
    Param::new("vars", ParamType::Mixed)
        .nullable()
        .optional()
        .by_ref()
        .variadic(),
];

/// `mb_convert_variables(string $to_encoding, array|string $from_encoding,
/// mixed &$var, mixed &...$vars): string|false`: one source encoding is
/// guessed from all the strings together, and returned.
fn mb_convert_variables(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let to = encoding::resolve(
        &String::from_utf8_lossy(args::bytes(a, 0)),
        "mb_convert_variables",
        1,
        "to_encoding",
    )?;
    let candidates = encoding_list(
        ctx,
        args::value(a, 1),
        "mb_convert_variables",
        2,
        "from_encoding",
    )?;
    let mut combined = Vec::new();
    for variable in &a[2..] {
        let Ok(strings) = walk::strings(variable.clone()) else {
            ctx.warning("mb_convert_variables(): Cannot handle recursive references".into());
            return Ok(Value::Bool(false));
        };
        for string in strings.values {
            combined.extend_from_slice(string.as_bytes());
        }
    }
    let Some(from) = guess(&combined, &candidates, false) else {
        return Ok(Value::Bool(false));
    };
    for (index, variable) in a.iter().enumerate().skip(2) {
        let converted = convert_value(ctx, variable.deref().clone(), to, &[from])?;
        if let Some(cell) = args::reference(a, index) {
            *cell.borrow_mut() = converted;
        }
    }
    Ok(Value::from(from.name()))
}

/// An encoding list as PHP accepts it: an array of names, or names separated
/// by commas, where `"auto"` stands for the detect order. Null is the detect
/// order itself.
fn encoding_list(
    ctx: &mut dyn Context,
    value: &Value,
    function: &str,
    number: usize,
    parameter: &str,
) -> Result<Vec<Encoding>, NativeError> {
    let detect_order = ctx.state().get_or_default::<Settings>().detect.clone();
    let names: Vec<String> = match value {
        Value::Null => return Ok(detect_order),
        Value::Array(array) => array
            .iter()
            .map(|(_, name)| crate::string_of(name))
            .collect(),
        Value::String(names) => names
            .to_string_lossy()
            .split(',')
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    };
    let mut list = Vec::new();
    for name in &names {
        let name = name.trim();
        if name.eq_ignore_ascii_case("auto") {
            list.extend_from_slice(&detect_order);
        } else if let Some(encoding) = Encoding::find(name) {
            list.push(encoding);
        } else if !name.is_empty() {
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #{number} (${parameter}) \
                 contains invalid encoding \"{name}\""
            )));
        }
    }
    if list.is_empty() {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{number} (${parameter}) must specify at least one encoding"
        )));
    }
    Ok(list)
}
