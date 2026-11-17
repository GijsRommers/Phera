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
//! `iconv-native` gives safe access to the platform's iconv. It converts a
//! whole string per call, which is CPU work without waiting, like any other
//! string function.

mod mime;

use iconv_native::ConvertError;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::encoding::read_u32;
use crate::text::bounds;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("iconv", ICONV, iconv));
    registry.function(FunctionEntry::sync(
        "iconv_strlen",
        ICONV_STRLEN,
        iconv_strlen,
    ));
    registry.function(FunctionEntry::sync(
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
    registry.function(FunctionEntry::sync(
        "iconv_get_encoding",
        ICONV_GET_ENCODING,
        iconv_get_encoding,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_set_encoding",
        ICONV_SET_ENCODING,
        iconv_set_encoding,
    ));
    mime::register(registry);
    registry.constant("ICONV_MIME_DECODE_STRICT", mime::DECODE_STRICT);
    registry.constant(
        "ICONV_MIME_DECODE_CONTINUE_ON_ERROR",
        mime::DECODE_CONTINUE_ON_ERROR,
    );
}

/// The `iconv.*_encoding` settings, which `iconv_set_encoding` changes for
/// the rest of the request.
struct Settings {
    input: String,
    output: String,
    internal: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            input: "UTF-8".into(),
            output: "UTF-8".into(),
            internal: "UTF-8".into(),
        }
    }
}

#[derive(Clone, Copy)]
enum Setting {
    Input,
    Output,
    Internal,
}

impl Setting {
    const ALL: [Setting; 3] = [Self::Input, Self::Output, Self::Internal];

    fn name(self) -> &'static str {
        match self {
            Self::Input => "input_encoding",
            Self::Output => "output_encoding",
            Self::Internal => "internal_encoding",
        }
    }

    fn find(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|setting| setting.name().eq_ignore_ascii_case(name))
    }

    fn of(self, settings: &mut Settings) -> &mut String {
        match self {
            Self::Input => &mut settings.input,
            Self::Output => &mut settings.output,
            Self::Internal => &mut settings.internal,
        }
    }
}

/// `EINVAL`, which is 22 on Linux and macOS: iconv stopped at an
/// incomplete character at the end of the input, rather than an invalid one
/// (`EILSEQ`).
const EINVAL: i32 = 22;

/// Whether the iconv call that just failed stopped at an incomplete
/// character. `iconv-native` returns right after `iconv()` fails; all it
/// does in between is shrink its buffer and close the converter, which do
/// not set `errno` when they succeed.
fn stopped_at_incomplete_character() -> bool {
    std::io::Error::last_os_error().raw_os_error() == Some(EINVAL)
}

/// Converts with PHP's errors: a warning for an unknown encoding, a notice
/// for bad input, and `None` for both.
fn convert(
    ctx: &mut dyn Context,
    bytes: &[u8],
    from: &str,
    to: &str,
    function: &str,
) -> Option<Vec<u8>> {
    let result = if to.to_ascii_uppercase().contains("//IGNORE") {
        // PHP returns what was converted unless the input ends in an
        // incomplete character. The probe must use the real target: glibc
        // reports EILSEQ instead of EINVAL once it has skipped a character
        // the target cannot hold, and PHP then returns the string.
        let probe = iconv_native::convert(bytes, from, to);
        if matches!(probe, Err(ConvertError::InvalidInput)) && stopped_at_incomplete_character() {
            probe
        } else {
            iconv_native::convert_lossy(bytes, from, to).map_err(Into::into)
        }
    } else {
        iconv_native::convert(bytes, from, to)
    };
    match result {
        Ok(bytes) => Some(bytes),
        Err(ConvertError::UnknownConversion) => {
            ctx.warning(format!(
                "{function}(): Wrong encoding, conversion from \"{from}\" to \"{to}\" is not allowed"
            ));
            None
        }
        Err(ConvertError::InvalidInput) => {
            let problem = if stopped_at_incomplete_character() {
                "incomplete multibyte character"
            } else {
                "illegal character"
            };
            ctx.notice(format!(
                "{function}(): Detected an {problem} in input string"
            ));
            None
        }
    }
}

/// The code points of `bytes`, through UCS-4LE.
fn code_points(
    ctx: &mut dyn Context,
    bytes: &[u8],
    encoding: &str,
    function: &str,
) -> Option<Vec<u32>> {
    let ucs4 = convert(ctx, bytes, encoding, "UCS-4LE", function)?;
    Some(points_of_ucs4(&ucs4))
}

fn points_of_ucs4(ucs4: &[u8]) -> Vec<u32> {
    ucs4.as_chunks::<4>()
        .0
        .iter()
        .map(|point| read_u32(point))
        .collect()
}

/// The encoding argument at `index`, or `iconv.internal_encoding`.
fn encoding_argument(ctx: &mut dyn Context, a: &[Value], index: usize) -> String {
    match args::string(a, index) {
        Some(name) => name.to_string_lossy(),
        None => ctx.state().get_or_default::<Settings>().internal.clone(),
    }
}

/// `iconv_mime_*` take this as their default target.
fn internal_encoding(ctx: &mut dyn Context) -> String {
    ctx.state().get_or_default::<Settings>().internal.clone()
}

const ENCODING: Param = crate::ENCODING;

const ICONV: &[Param] = &[
    Param::new("from_encoding", ParamType::String),
    Param::new("to_encoding", ParamType::String),
    Param::new("string", ParamType::String),
];

/// `iconv(string $from_encoding, string $to_encoding, string $string): string|false`
fn iconv(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let from = String::from_utf8_lossy(args::bytes(a, 0));
    let to = String::from_utf8_lossy(args::bytes(a, 1));
    let converted = convert(ctx, args::bytes(a, 2), &from, &to, "iconv");
    Ok(converted.map_or(Value::Bool(false), Value::from))
}

const ICONV_STRLEN: &[Param] = &[crate::STRING, ENCODING];

/// `iconv_strlen(string $string, ?string $encoding = null): int|false`
fn iconv_strlen(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding_argument(ctx, a, 1);
    let points = code_points(ctx, args::bytes(a, 0), &encoding, "iconv_strlen");
    Ok(points.map_or(Value::Bool(false), |points| Value::Int(points.len() as i64)))
}

const ICONV_SUBSTR: &[Param] = &[
    crate::STRING,
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    ENCODING,
];

/// `iconv_substr(string $string, int $offset, ?int $length = null, ?string $encoding = null): string|false`
fn iconv_substr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding_argument(ctx, a, 3);
    let Some(points) = code_points(ctx, args::bytes(a, 0), &encoding, "iconv_substr") else {
        return Ok(Value::Bool(false));
    };
    let (start, end) = bounds(points.len(), args::int(a, 1).unwrap_or(0), args::int(a, 2));
    let ucs4: Vec<u8> = points[start..end]
        .iter()
        .flat_map(|point| point.to_le_bytes())
        .collect();
    let substring = convert(ctx, &ucs4, "UCS-4LE", &encoding, "iconv_substr");
    Ok(substring.map_or(Value::Bool(false), Value::from))
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

const ICONV_GET_ENCODING: &[Param] = &[Param::new("type", ParamType::String).optional()];

/// `iconv_get_encoding(string $type = "all"): array|string|false`
fn iconv_get_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let kind = args::string(a, 0).map_or_else(|| "all".into(), |kind| kind.to_string_lossy());
    let settings = ctx.state().get_or_default::<Settings>();
    if kind.eq_ignore_ascii_case("all") {
        let mut all = PhpArray::new();
        for setting in Setting::ALL {
            let value = Value::from(setting.of(settings).clone());
            all.insert(ArrayKey::str(setting.name()), value);
        }
        return Ok(Value::from(all));
    }
    Ok(Setting::find(&kind).map_or(Value::Bool(false), |setting| {
        Value::from(setting.of(settings).clone())
    }))
}

const ICONV_SET_ENCODING: &[Param] = &[
    Param::new("type", ParamType::String),
    Param::new("encoding", ParamType::String),
];

/// `iconv_set_encoding(string $type, string $encoding): bool`, deprecated
/// like the settings themselves.
fn iconv_set_encoding(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let Some(setting) = Setting::find(&String::from_utf8_lossy(args::bytes(a, 0))) else {
        return Ok(Value::Bool(false));
    };
    ctx.deprecated(format!(
        "iconv_set_encoding(): Use of iconv.{} is deprecated",
        setting.name()
    ));
    let encoding = String::from_utf8_lossy(args::bytes(a, 1)).into_owned();
    *setting.of(ctx.state().get_or_default::<Settings>()) = encoding;
    Ok(Value::Bool(true))
}
