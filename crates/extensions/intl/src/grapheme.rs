//! Work with graphemes while retaining the original PHP string bytes.
//!
//! ```php
//! grapheme_strlen("👩‍💻é");          // 2
//! grapheme_substr("👩‍💻é", 0, 1);    // "👩‍💻"
//! grapheme_extract("👩‍💻é", 1, offset: 11); // "é", offset in bytes
//! ```

use icu_segmenter::GraphemeClusterSegmenter;
use phera_extension_api::{ArrayKey, ConstantValue, Context, FunctionEntry, NativeError};
use phera_extension_api::{NativeResult, Param, ParamType, PhpArray, Registry, Value, args};

use crate::{error, grapheme_search};

pub const LIMIT: i64 = 1_073_741_823;

pub fn boundaries(text: &[u8]) -> impl Iterator<Item = usize> {
    GraphemeClusterSegmenter::new_neo().segment_utf8(text)
}

fn length(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let text = args::bytes(arguments, 0);
    if std::str::from_utf8(text).is_err() {
        return input_error(context, "grapheme_strlen", Value::Null);
    }
    Ok(Value::Int(boundaries(text).count().saturating_sub(1) as i64))
}

fn substring(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    check_locale(arguments, 3, "grapheme_substr")?;
    let text = args::bytes(arguments, 0);
    let offset = args::int(arguments, 1).unwrap_or(0);
    let length = args::int(arguments, 2).unwrap_or(text.len() as i64);
    for (number, name, value) in [(2, "offset", offset), (3, "length", length)] {
        if i32::try_from(value).is_err() {
            return Err(NativeError::value_error(format!(
                "grapheme_substr(): Argument #{number} (${name}) is too large"
            )));
        }
    }
    if std::str::from_utf8(text).is_err() {
        return input_error(context, "grapheme_substr", Value::Bool(false));
    }
    let ends: Vec<_> = boundaries(text).collect();
    let count = ends.len().saturating_sub(1) as i64;
    let from = relative(offset, count).clamp(0, count);
    let to = if length < 0 {
        count + length
    } else {
        from + length
    };
    let to = to.clamp(from, count);
    Ok(Value::from(
        text[ends[from as usize]..ends[to as usize]].to_vec(),
    ))
}

fn split(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let text = args::bytes(arguments, 0);
    let length = args::int(arguments, 1).unwrap_or(1);
    if !(1..=LIMIT).contains(&length) {
        return Err(NativeError::value_error(format!(
            "grapheme_str_split(): Argument #2 ($length) must be greater than 0 \
             and less than or equal to {LIMIT}"
        )));
    }
    let mut start = 0;
    let mut chunks = boundaries(text)
        .skip(length as usize)
        .step_by(length as usize)
        .map(|end| {
            let part = Value::from(text[start..end].to_vec());
            start = end;
            part
        })
        .collect::<Vec<_>>();
    if start < text.len() {
        chunks.push(Value::from(text[start..].to_vec()));
    }
    let array: PhpArray = chunks
        .into_iter()
        .enumerate()
        .map(|(key, value)| (ArrayKey::Int(key as i64), value))
        .collect();
    Ok(Value::from(array))
}

enum ExtractUnit {
    Grapheme,
    Byte,
    Scalar,
}

fn extract(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    const FUNCTION: &str = "grapheme_extract";
    let text = args::bytes(arguments, 0);
    let offset = relative(args::int(arguments, 3).unwrap_or(0), text.len() as i64);
    assign_next(arguments, offset)?;
    let unit = match args::int(arguments, 2).unwrap_or(0) {
        0 => ExtractUnit::Grapheme,
        1 => ExtractUnit::Byte,
        2 => ExtractUnit::Scalar,
        _ => {
            return Err(NativeError::value_error(
                "grapheme_extract(): Argument #3 ($type) must be one of GRAPHEME_EXTR_COUNT, \
                 GRAPHEME_EXTR_MAXBYTES, or GRAPHEME_EXTR_MAXCHARS",
            ));
        }
    };
    let Ok(start) = i32::try_from(offset) else {
        return extract_error(context, "start not contained in string");
    };
    if start < 0 || start as usize >= text.len() {
        return extract_error(context, "start not contained in string");
    }
    let size = args::int(arguments, 1).unwrap_or(0);
    let validation = match size {
        size if size < 0 => Some("must be greater than or equal to 0"),
        size if i32::try_from(size).is_err() => Some("is too large"),
        _ => None,
    };
    if let Some(detail) = validation {
        return Err(NativeError::value_error(format!(
            "{FUNCTION}(): Argument #2 ($size) {detail}"
        )));
    }
    if size == 0 {
        return Ok(Value::from(""));
    }
    let start = start as usize;
    let Some(skip) = text[start..]
        .iter()
        .position(|&byte| byte.is_ascii() || (0xc2..=0xf4).contains(&byte))
    else {
        return extract_error(context, "grapheme_extract: invalid input string");
    };
    let start = start + skip;
    let rest = &text[start..];
    let end = extract_end(rest, size as usize, unit);
    assign_next(arguments, (start + end) as i64)?;
    Ok(Value::from(rest[..end].to_vec()))
}

fn extract_end(text: &[u8], size: usize, unit: ExtractUnit) -> usize {
    let mut consumed = 0;
    let mut previous = 0;
    for boundary in boundaries(text).skip(1) {
        let step = match unit {
            ExtractUnit::Grapheme => 1,
            ExtractUnit::Byte => boundary - previous,
            ExtractUnit::Scalar => String::from_utf8_lossy(&text[previous..boundary])
                .chars()
                .count(),
        };
        consumed += step;
        if consumed > size {
            break;
        }
        previous = boundary;
    }
    previous
}

fn extract_error(context: &mut dyn Context, detail: &str) -> NativeResult {
    error::fail(
        context,
        "grapheme_extract",
        error::ILLEGAL_ARGUMENT,
        detail,
        Value::Bool(false),
    )
}

fn assign_next(arguments: &[Value], offset: i64) -> Result<(), NativeError> {
    if let Some(reference) = args::reference(arguments, 4) {
        *reference.try_borrow_mut().map_err(|_| {
            NativeError::error("grapheme_extract(): Argument #5 is already being modified")
        })? = Value::Int(offset);
    }
    Ok(())
}

pub fn relative(offset: i64, length: i64) -> i64 {
    if offset < 0 { length + offset } else { offset }
}

pub fn input_error(context: &mut dyn Context, function: &str, result: Value) -> NativeResult {
    error::fail(
        context,
        function,
        error::INVALID_CHAR,
        "Error converting input string to UTF-16",
        result,
    )
}

pub fn check_locale(arguments: &[Value], index: usize, function: &str) -> Result<(), NativeError> {
    if args::bytes(arguments, index).contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #{} ($locale) must not contain any null bytes",
            index + 1
        )));
    }
    Ok(())
}

pub const LOCALE: Param =
    Param::new("locale", ParamType::String).with_default(ConstantValue::String(""));

pub(crate) fn register(registry: &mut Registry) {
    const STRING: Param = Param::new("string", ParamType::String);
    const LENGTH: Param = Param::new("length", ParamType::Int);
    const SUBSTRING: &[Param] = &[
        STRING,
        Param::new("offset", ParamType::Int),
        LENGTH.nullable().with_default(ConstantValue::Null),
        LOCALE,
    ];
    const SPLIT: &[Param] = &[STRING, LENGTH.with_default(ConstantValue::Int(1))];
    const EXTRACT: &[Param] = &[
        Param::new("haystack", ParamType::String),
        Param::new("size", ParamType::Int),
        Param::new("type", ParamType::Int)
            .with_constant_default("GRAPHEME_EXTR_COUNT", ConstantValue::Int(0)),
        Param::new("offset", ParamType::Int).with_default(ConstantValue::Int(0)),
        Param::new("next", ParamType::Mixed)
            .by_ref()
            .with_default(ConstantValue::Null),
    ];
    for (entry, result) in [
        (
            FunctionEntry::sync("grapheme_strlen", &[STRING], length),
            "int|false|null",
        ),
        (
            FunctionEntry::sync("grapheme_substr", SUBSTRING, substring),
            "string|false",
        ),
        (
            FunctionEntry::sync("grapheme_str_split", SPLIT, split),
            "array|false",
        ),
        (
            FunctionEntry::sync("grapheme_extract", EXTRACT, extract),
            "string|false",
        ),
    ] {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
    grapheme_search::register(registry);
}
