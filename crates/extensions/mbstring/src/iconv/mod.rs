//! The iconv extension, which converts with the operating system's iconv
//! as PHP does, so both give the same results on the same machine:
//!
//! ```php
//! iconv('UTF-8', 'ISO-8859-1', 'é');       // "\xE9"
//! iconv('UTF-8', 'ASCII//IGNORE', 'aéb');  // "ab"
//! iconv('UTF-8', 'ASCII', "\xFF");         // false, Notice: Detected an illegal character
//! iconv_strlen('aé');                      // 2
//! iconv_strpos('aéb', 'b');                // 2
//! ```
//!
//! Unlike mbstring, iconv knows whatever encodings the platform has, under
//! the platform's names: glibc on Linux, Apple's libiconv on macOS. Their
//! results differ from each other in places, and so do PHP's.
//!
//! `iconv-native` gives safe access to the platform's iconv. A conversion
//! is CPU work without waiting, but a large one can take long (a megabyte
//! to `//IGNORE` takes a quarter of a second on macOS), so `iconv`,
//! `iconv_strlen` and `iconv_substr` convert large strings on Tokio's
//! blocking threads instead of the request's worker.

mod convert;
mod mime;
mod settings;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

use self::convert::{code_points, convert_text, points_of_ucs4};
use self::settings::encoding_argument;
use crate::text::bounds;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous("iconv", ICONV, iconv));
    registry.function(FunctionEntry::asynchronous(
        "iconv_strlen",
        ICONV_STRLEN,
        iconv_strlen,
    ));
    registry.function(FunctionEntry::asynchronous(
        "iconv_substr",
        ICONV_SUBSTR,
        iconv_substr,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_strpos",
        ICONV_STRPOS,
        iconv_strpos,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_strrpos",
        ICONV_STRRPOS,
        iconv_strrpos,
    ));
    settings::register(registry);
    mime::register(registry);
    registry.constant("ICONV_MIME_DECODE_STRICT", mime::DECODE_STRICT);
    registry.constant(
        "ICONV_MIME_DECODE_CONTINUE_ON_ERROR",
        mime::DECODE_CONTINUE_ON_ERROR,
    );
}

const ENCODING: Param = crate::ENCODING;

const ICONV: &[Param] = &[
    Param::new("from_encoding", ParamType::String),
    Param::new("to_encoding", ParamType::String),
    Param::new("string", ParamType::String),
];

/// `iconv(string $from_encoding, string $to_encoding, string $string): string|false`
fn iconv(ctx: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let from = String::from_utf8_lossy(args::bytes(&a, 0));
        let to = String::from_utf8_lossy(args::bytes(&a, 1));
        let converted = convert_text(ctx, args::bytes(&a, 2), &from, &to, "iconv").await?;
        Ok(converted.map_or(Value::Bool(false), Value::from))
    })
}

const ICONV_STRLEN: &[Param] = &[crate::STRING, ENCODING];

/// `iconv_strlen(string $string, ?string $encoding = null): int|false`
fn iconv_strlen(ctx: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let encoding = encoding_argument(ctx, &a, 1);
        let ucs4 = convert_text(
            ctx,
            args::bytes(&a, 0),
            &encoding,
            "UCS-4LE",
            "iconv_strlen",
        )
        .await?;
        Ok(ucs4.map_or(Value::Bool(false), |ucs4| {
            Value::Int((ucs4.len() / 4) as i64)
        }))
    })
}

const ICONV_SUBSTR: &[Param] = &[
    crate::STRING,
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    ENCODING,
];

/// `iconv_substr(string $string, int $offset, ?int $length = null, ?string $encoding = null): string|false`
fn iconv_substr(ctx: &mut dyn Context, a: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "iconv_substr";
        let encoding = encoding_argument(ctx, &a, 3);
        let Some(ucs4) =
            convert_text(ctx, args::bytes(&a, 0), &encoding, "UCS-4LE", function).await?
        else {
            return Ok(Value::Bool(false));
        };
        let characters = ucs4.len() / 4;
        let (start, end) = bounds(characters, args::int(&a, 1).unwrap_or(0), args::int(&a, 2));
        let selected = &ucs4[start * 4..end * 4];
        let substring = convert_text(ctx, selected, "UCS-4LE", &encoding, function).await?;
        Ok(substring.map_or(Value::Bool(false), Value::from))
    })
}

const ICONV_STRPOS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
    ENCODING,
];

/// `iconv_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false`
fn iconv_strpos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding_argument(ctx, a, 3);
    let offset = args::int(a, 2).unwrap_or(0);
    position(ctx, a, &encoding, offset, Direction::Forward)
}

const ICONV_STRRPOS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    ENCODING,
];

/// `iconv_strrpos(string $haystack, string $needle, ?string $encoding = null): int|false`
fn iconv_strrpos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding_argument(ctx, a, 2);
    position(ctx, a, &encoding, 0, Direction::Backward)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Forward,
    Backward,
}

/// PHP reports problems in the haystack only at certain points of the
/// search, which is why the haystack is converted again, for its notice,
/// where PHP would notice it.
fn position(
    ctx: &mut dyn Context,
    a: &[Value],
    encoding: &str,
    offset: i64,
    direction: Direction,
) -> NativeResult {
    let function = match direction {
        Direction::Forward => "iconv_strpos",
        Direction::Backward => "iconv_strrpos",
    };
    let haystack = args::bytes(a, 0);
    let haystack_is_bad =
        |ctx: &mut dyn Context| code_points(ctx, haystack, encoding, function).is_none();
    if offset < 0 && haystack_is_bad(ctx) {
        return Ok(Value::Bool(false));
    }
    // The haystack up to its first invalid sequence: the `//` suffix keeps
    // iconv-native's lossy conversion from adding //IGNORE//TRANSLIT.
    let Ok(ucs4) = iconv_native::convert_lossy(haystack, encoding, "UCS-4LE//") else {
        haystack_is_bad(ctx);
        return Ok(Value::Bool(false));
    };
    let points = points_of_ucs4(&ucs4);
    let length = points.len() as i64;
    if offset < -length {
        return Err(offset_error(function));
    }
    let Some(needle) = code_points(ctx, args::bytes(a, 1), encoding, function) else {
        return Ok(Value::Bool(false));
    };
    if offset > length {
        if !points.is_empty() && haystack_is_bad(ctx) {
            return Ok(Value::Bool(false));
        }
        return Err(offset_error(function));
    }
    if needle.is_empty() {
        return Ok(Value::Bool(false));
    }
    let start = if offset < 0 {
        points.len() - offset.unsigned_abs() as usize
    } else {
        offset as usize
    };
    let mut matches = (start..points.len()).filter(|&index| points[index..].starts_with(&needle));
    let found = match direction {
        Direction::Forward => matches.next(),
        Direction::Backward => matches.next_back(),
    };
    if !points.is_empty()
        && (direction == Direction::Backward || found.is_none())
        && haystack_is_bad(ctx)
    {
        return Ok(Value::Bool(false));
    }
    Ok(found.map_or(Value::Bool(false), |index| Value::Int(index as i64)))
}

fn offset_error(function: &str) -> NativeError {
    NativeError::value_error(format!(
        "{function}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    ))
}
