//! Padding and line breaks: `str_pad("7", 3, "0", STR_PAD_LEFT)` is `"007"`.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::buffer;
use crate::constants::{STR_PAD_BOTH, STR_PAD_LEFT, STR_PAD_RIGHT};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[STR_PAD, NL2BR];

/// `str_pad(string $string, int $length, string $pad_string = " ",
/// int $pad_type = STR_PAD_RIGHT): string`
///
/// ```php
/// str_pad("7", 3, "0", STR_PAD_LEFT); // "007"
/// str_pad("ab", 7, "-=", STR_PAD_BOTH); // "-=ab-=-": the padding restarts on each side
/// ```
const STR_PAD: FunctionEntry = FunctionEntry::sync(
    "str_pad",
    &[
        Param::new("string", ParamType::String),
        Param::new("length", ParamType::Int),
        Param::new("pad_string", ParamType::String).optional(),
        Param::new("pad_type", ParamType::Int).optional(),
    ],
    str_pad,
);

fn str_pad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let length = args::int(arguments, 1).unwrap_or_default();
    // PHP returns a string that is long enough as it is, before it looks at
    // the other two arguments: str_pad("abc", 2, "") is no error.
    if length <= string.len() as i64 {
        return Ok(arguments[0].clone());
    }
    let padding = if args::given(arguments, 2) {
        args::bytes(arguments, 2)
    } else {
        b" "
    };
    if padding.is_empty() {
        return Err(NativeError::value_error(
            "str_pad(): Argument #3 ($pad_string) must not be empty",
        ));
    }
    let length = length as usize;
    let missing = length - string.len();
    let before = match args::int(arguments, 3).unwrap_or(STR_PAD_RIGHT) {
        STR_PAD_LEFT => missing,
        STR_PAD_RIGHT => 0,
        STR_PAD_BOTH => missing / 2,
        _ => {
            return Err(NativeError::value_error(
                "str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH",
            ));
        }
    };
    let mut result = buffer(length)?;
    result.extend(padding.iter().cycle().take(before));
    result.extend_from_slice(string);
    result.extend(padding.iter().cycle().take(missing - before));
    Ok(Value::from(result))
}

/// `nl2br(string $string, bool $use_xhtml = true): string`, a `<br />`
/// before every line break.
///
/// ```php
/// nl2br("a\nb");          // "a<br />\nb"
/// nl2br("a\r\nb", false); // "a<br>\r\nb": "\r\n" and "\n\r" are one break
/// ```
const NL2BR: FunctionEntry = FunctionEntry::sync(
    "nl2br",
    &[
        Param::new("string", ParamType::String),
        Param::new("use_xhtml", ParamType::Bool).optional(),
    ],
    nl2br,
);

fn nl2br(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let tag: &[u8] = if args::bool(arguments, 1).unwrap_or(true) {
        b"<br />"
    } else {
        b"<br>"
    };
    let mut result = buffer(string.len().saturating_mul(7))?;
    let mut cursor = 0;
    while cursor < string.len() {
        let byte = string[cursor];
        cursor += 1;
        let line_break = matches!(byte, b'\r' | b'\n');
        if line_break {
            result.extend_from_slice(tag);
        }
        result.push(byte);
        if line_break
            && let Some(&next) = string.get(cursor)
            && matches!(next, b'\r' | b'\n')
            && next != byte
        {
            result.push(next);
            cursor += 1;
        }
    }
    Ok(Value::from(result))
}
