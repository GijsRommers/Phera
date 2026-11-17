//! Backslash escapes: `addslashes("O'Reilly")` is `"O\'Reilly"`,
//! `stripcslashes('\x41\n')` is `"A"` and a line break.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;
use super::trim::character_mask;
use super::url::hex_digit;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ADDSLASHES,
    STRIPSLASHES,
    ADDCSLASHES,
    STRIPCSLASHES,
    QUOTEMETA,
];

/// The C escapes that `addcslashes` writes and `stripcslashes` reads, as
/// (byte, letter after the backslash).
const NAMED_ESCAPES: [(u8, u8); 7] = [
    (b'\n', b'n'),
    (b'\r', b'r'),
    (b'\t', b't'),
    (0x07, b'a'),
    (0x0b, b'v'),
    (0x08, b'b'),
    (0x0c, b'f'),
];

/// `addslashes(string $string): string`, a backslash before `'`, `"`, `\`
/// and NUL, which becomes `\0`.
///
/// ```php
/// addslashes("O'Reilly"); // "O\'Reilly"
/// ```
const ADDSLASHES: FunctionEntry = FunctionEntry::sync(
    "addslashes",
    &[Param::new("string", ParamType::String)],
    addslashes,
);

fn addslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        match byte {
            0 => result.extend_from_slice(b"\\0"),
            b'\'' | b'"' | b'\\' => result.extend_from_slice(&[b'\\', byte]),
            _ => result.push(byte),
        }
    }
    Ok(Value::from(result))
}

/// `stripslashes(string $string): string`, the reverse of `addslashes`.
///
/// Any backslash goes and keeps the byte after it; `\0` is NUL and a
/// backslash at the very end disappears.
///
/// ```php
/// stripslashes("O\\'Reilly"); // "O'Reilly"
/// ```
const STRIPSLASHES: FunctionEntry = FunctionEntry::sync(
    "stripslashes",
    &[Param::new("string", ParamType::String)],
    stripslashes,
);

fn stripslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len())?;
    let mut bytes = string.iter();
    while let Some(&byte) = bytes.next() {
        if byte != b'\\' {
            result.push(byte);
        } else if let Some(&escaped) = bytes.next() {
            result.push(if escaped == b'0' { 0 } else { escaped });
        }
    }
    Ok(Value::from(result))
}

/// `quotemeta(string $string): string`, a backslash before each of
/// `. \ + * ? [ ^ ] ( $ )`.
///
/// ```php
/// quotemeta("1+1=2?"); // "1\+1=2\?"
/// ```
const QUOTEMETA: FunctionEntry = FunctionEntry::sync(
    "quotemeta",
    &[Param::new("string", ParamType::String)],
    quotemeta,
);

fn quotemeta(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        if b".\\+*?[^]($)".contains(&byte) {
            result.push(b'\\');
        }
        result.push(byte);
    }
    Ok(Value::from(result))
}

/// `addcslashes(string $string, string $characters): string`, C-style
/// escapes for the listed characters (ranges such as `"\0..\37"` allowed).
///
/// Control characters get their C name (`\n`, `\t`, …) or three octal
/// digits; other characters just a backslash.
///
/// ```php
/// addcslashes("foo[bar]", "A..Z"); // "foo[bar]"
/// addcslashes("a\tb\x01", "\0..\37"); // 'a\tb\001'
/// ```
const ADDCSLASHES: FunctionEntry = FunctionEntry::sync(
    "addcslashes",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String),
    ],
    addcslashes,
);

fn addcslashes(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    // PHP returns an empty string before reading the list, so a bad range
    // in it does not warn.
    if string.is_empty() {
        return Ok(Value::from(""));
    }
    let escape = character_mask(context, "addcslashes", args::bytes(arguments, 1));
    let mut result = buffer(string.len().saturating_mul(4))?;
    for &byte in string {
        if !escape[usize::from(byte)] {
            result.push(byte);
            continue;
        }
        result.push(b'\\');
        if let Some(&(_, letter)) = NAMED_ESCAPES.iter().find(|(named, _)| *named == byte) {
            result.push(letter);
        } else if (32..=126).contains(&byte) {
            result.push(byte);
        } else {
            result.extend_from_slice(&[
                b'0' + (byte >> 6),
                b'0' + ((byte >> 3) & 7),
                b'0' + (byte & 7),
            ]);
        }
    }
    Ok(Value::from(result))
}

/// `stripcslashes(string $string): string`, reading C-style escapes: the
/// named ones, `\x` with one or two hexadecimal digits, and one to three
/// octal digits (wrapping above 255).
///
/// ```php
/// stripcslashes('\x41\102\n'); // "AB" and a line break
/// stripcslashes('\q');         // "q"
/// ```
const STRIPCSLASHES: FunctionEntry = FunctionEntry::sync(
    "stripcslashes",
    &[Param::new("string", ParamType::String)],
    stripcslashes,
);

fn stripcslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while cursor < string.len() {
        let byte = string[cursor];
        cursor += 1;
        if byte != b'\\' {
            result.push(byte);
            continue;
        }
        // A backslash at the end stays.
        let Some(&escaped) = string.get(cursor) else {
            result.push(byte);
            break;
        };
        cursor += 1;
        if let Some(&(named, _)) = NAMED_ESCAPES.iter().find(|(_, letter)| *letter == escaped) {
            result.push(named);
        } else {
            result.push(numeric_escape(escaped, string, &mut cursor));
        }
    }
    Ok(Value::from(result))
}

/// The byte a `\x41` or `\101` escape stands for, consuming its digits
/// after `cursor`; any other escaped byte stands for itself.
fn numeric_escape(escaped: u8, string: &[u8], cursor: &mut usize) -> u8 {
    if escaped == b'x' && string.get(*cursor).copied().and_then(hex_digit).is_some() {
        // `\x` and one or two hexadecimal digits.
        add_digits(0, 16, hex_digit, string, cursor)
    } else if let Some(first) = octal_digit(escaped) {
        // One to three octal digits; a value above 255 wraps around.
        add_digits(first, 8, octal_digit, string, cursor)
    } else {
        escaped
    }
}

/// `number` followed by up to two more digits from `cursor` on.
fn add_digits(
    mut number: u8,
    radix: u8,
    digit: fn(u8) -> Option<u8>,
    string: &[u8],
    cursor: &mut usize,
) -> u8 {
    for _ in 0..2 {
        let Some(value) = string.get(*cursor).copied().and_then(digit) else {
            break;
        };
        number = number.wrapping_mul(radix).wrapping_add(value);
        *cursor += 1;
    }
    number
}

fn octal_digit(byte: u8) -> Option<u8> {
    (b'0'..=b'7').contains(&byte).then(|| byte - b'0')
}
