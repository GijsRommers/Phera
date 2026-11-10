use std::fmt::Write;

use phera_extension_api::{Context, NativeError, NativeResult, Value};

pub const HEX_TAG: i64 = 1;
pub const HEX_AMP: i64 = 2;
pub const HEX_APOS: i64 = 4;
pub const HEX_QUOT: i64 = 8;
pub const UNESCAPED_SLASHES: i64 = 64;
pub const UNESCAPED_UNICODE: i64 = 256;
pub const PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub const PRESERVE_ZERO_FRACTION: i64 = 1024;
pub const UNESCAPED_LINE_TERMINATORS: i64 = 2048;
pub const INVALID_UTF8_IGNORE: i64 = 1 << 20;
pub const INVALID_UTF8_SUBSTITUTE: i64 = 1 << 21;
pub const THROW_ON_ERROR: i64 = 1 << 22;

enum EncodeError {
    Utf8,
    InfOrNan,
    UnsupportedType,
}

impl EncodeError {
    fn message(&self) -> &'static str {
        match self {
            EncodeError::Utf8 => "Malformed UTF-8 characters, possibly incorrectly encoded",
            EncodeError::InfOrNan => "Inf and NaN cannot be JSON encoded",
            EncodeError::UnsupportedType => "Type is not supported",
        }
    }

    /// What `JSON_PARTIAL_OUTPUT_ON_ERROR` writes in place of the bad value.
    fn substitute(&self) -> &'static str {
        match self {
            EncodeError::Utf8 | EncodeError::UnsupportedType => "null",
            EncodeError::InfOrNan => "0",
        }
    }
}

/// `json_encode(mixed $value, int $flags = 0, int $depth = 512): string|false`
///
/// Arrays and objects are not encoded yet, and `$depth` is unused until they
/// are. Failures don't update `json_last_error()` yet, which needs per-request
/// state.
pub fn json_encode(_context: &mut dyn Context, args: &[Value]) -> NativeResult {
    let flags = int_argument(args, 2, "flags", 0)?;
    int_argument(args, 3, "depth", 512)?;

    let encoded = match args.first().map(|value| value.deref()).as_deref() {
        None | Some(Value::Null) => Ok("null".to_owned()),
        Some(Value::Bool(b)) => Ok(b.to_string()),
        Some(Value::Int(i)) => Ok(i.to_string()),
        Some(Value::Float(f)) => encode_float(*f, flags),
        Some(Value::String(bytes)) => encode_string(bytes.as_bytes(), flags),
        Some(_) => Err(EncodeError::UnsupportedType),
    };

    match encoded {
        Ok(json) => Ok(Value::from(json)),
        Err(error) if flags & PARTIAL_OUTPUT_ON_ERROR != 0 => Ok(Value::from(error.substitute())),
        Err(error) if flags & THROW_ON_ERROR != 0 => {
            Err(NativeError::throw("JsonException", error.message()))
        }
        Err(_) => Ok(Value::Bool(false)),
    }
}

fn int_argument(
    args: &[Value],
    position: usize,
    name: &str,
    default: i64,
) -> Result<i64, NativeError> {
    match args.get(position - 1) {
        None => Ok(default),
        Some(Value::Int(i)) => Ok(*i),
        Some(other) => Err(NativeError::type_error(format!(
            "json_encode(): Argument #{position} (${name}) must be of type int, {} given",
            other.type_name()
        ))),
    }
}

/// Matches PHP's `serialize_precision = -1`: the shortest round-tripping
/// digits, switching to exponent notation outside `1e-5 < |f| < 1e17`.
fn encode_float(f: f64, flags: i64) -> Result<String, EncodeError> {
    if !f.is_finite() {
        return Err(EncodeError::InfOrNan);
    }

    let scientific = format!("{f:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();

    if f != 0.0 && !(-4..17).contains(&exponent) {
        let point = if mantissa.contains('.') { "" } else { ".0" };
        let sign = if exponent < 0 { '-' } else { '+' };
        return Ok(format!("{mantissa}{point}e{sign}{}", exponent.abs()));
    }

    let mut out = f.to_string();
    if flags & PRESERVE_ZERO_FRACTION != 0 && !out.contains('.') {
        out.push_str(".0");
    }
    Ok(out)
}

fn encode_string(bytes: &[u8], flags: i64) -> Result<String, EncodeError> {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');

    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            escape_char(c, flags, &mut out);
        }
        if !chunk.invalid().is_empty() {
            if flags & INVALID_UTF8_IGNORE != 0 {
                continue;
            } else if flags & INVALID_UTF8_SUBSTITUTE != 0 {
                escape_char(char::REPLACEMENT_CHARACTER, flags, &mut out);
            } else {
                return Err(EncodeError::Utf8);
            }
        }
    }

    out.push('"');
    Ok(out)
}

fn escape_char(c: char, flags: i64, out: &mut String) {
    let has = |flag| flags & flag != 0;
    match c {
        '"' if has(HEX_QUOT) => out.push_str("\\u0022"),
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '/' if !has(UNESCAPED_SLASHES) => out.push_str("\\/"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '<' if has(HEX_TAG) => out.push_str("\\u003C"),
        '>' if has(HEX_TAG) => out.push_str("\\u003E"),
        '&' if has(HEX_AMP) => out.push_str("\\u0026"),
        '\'' if has(HEX_APOS) => out.push_str("\\u0027"),
        c if c < ' ' => write!(out, "\\u{:04x}", c as u32).unwrap(),
        '\u{2028}' | '\u{2029}' if has(UNESCAPED_UNICODE) && !has(UNESCAPED_LINE_TERMINATORS) => {
            write!(out, "\\u{:04x}", c as u32).unwrap()
        }
        c if c.is_ascii() || has(UNESCAPED_UNICODE) => out.push(c),
        c => {
            let mut units = [0; 2];
            for unit in c.encode_utf16(&mut units) {
                write!(out, "\\u{unit:04x}").unwrap();
            }
        }
    }
}
