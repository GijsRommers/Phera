//! Bytes as numbers: `bin2hex("ab")` is `"6162"`, `ord("a")` is 97,
//! `crc32("hello")` is 907060870.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;
use super::url::hex_digit;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[BIN2HEX, HEX2BIN, CRC32, ORD, CHR, UTF8_ENCODE];

/// `bin2hex(string $string): string`, two lowercase hexadecimal digits per
/// byte.
///
/// ```php
/// bin2hex("ab\0"); // "616200"
/// ```
const BIN2HEX: FunctionEntry = FunctionEntry::sync(
    "bin2hex",
    &[Param::new("string", ParamType::String)],
    bin2hex,
);

fn bin2hex(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        result.push(HEX[usize::from(byte >> 4)]);
        result.push(HEX[usize::from(byte & 0xf)]);
    }
    Ok(Value::from(result))
}

/// `hex2bin(string $string): string|false`, the reverse of `bin2hex`.
///
/// Bad input is a warning and `false`, not an exception.
///
/// ```php
/// hex2bin("6162"); // "ab"
/// hex2bin("616");  // false, with a warning about the odd length
/// ```
const HEX2BIN: FunctionEntry = FunctionEntry::sync(
    "hex2bin",
    &[Param::new("string", ParamType::String)],
    hex2bin,
);

fn hex2bin(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let (pairs, []) = string.as_chunks::<2>() else {
        context.warning("hex2bin(): Hexadecimal input string must have an even length".into());
        return Ok(Value::Bool(false));
    };
    let mut result = buffer(pairs.len())?;
    for &[high, low] in pairs {
        let (Some(high), Some(low)) = (hex_digit(high), hex_digit(low)) else {
            context.warning("hex2bin(): Input string must be hexadecimal string".into());
            return Ok(Value::Bool(false));
        };
        result.push(high * 16 + low);
    }
    Ok(Value::from(result))
}

/// `crc32(string $string): int`, the CRC-32 checksum of zlib and PNG, as an
/// unsigned number.
///
/// ```php
/// crc32("hello"); // 907060870
/// ```
const CRC32: FunctionEntry =
    FunctionEntry::sync("crc32", &[Param::new("string", ParamType::String)], crc32);

fn crc32(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    /// The CRC-32 polynomial, with its bits in reverse order.
    const POLYNOMIAL: u32 = 0xedb8_8320;
    let mut checksum = u32::MAX;
    for &byte in args::bytes(arguments, 0) {
        checksum ^= u32::from(byte);
        for _ in 0..8 {
            let low_bit_set = checksum & 1 != 0;
            checksum >>= 1;
            if low_bit_set {
                checksum ^= POLYNOMIAL;
            }
        }
    }
    Ok(Value::Int(i64::from(!checksum)))
}

/// `ord(string $character): int`, the first byte as a number.
///
/// PHP 8.5 deprecates passing anything but exactly one byte.
///
/// ```php
/// ord("a");  // 97
/// ord("ab"); // 97, with a deprecation
/// ```
const ORD: FunctionEntry =
    FunctionEntry::sync("ord", &[Param::new("character", ParamType::String)], ord);

fn ord(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    match string.len() {
        0 => context.deprecated("ord(): Providing an empty string is deprecated".into()),
        1 => {}
        _ => context.deprecated(
            "ord(): Providing a string that is not one byte long is deprecated. Use ord($str[0]) instead"
                .into(),
        ),
    }
    Ok(Value::Int(i64::from(string.first().copied().unwrap_or(0))))
}

/// `chr(int $codepoint): string`, a one-byte string.
///
/// PHP 8.5 deprecates numbers outside 0 to 255; the byte is the number
/// modulo 256 either way.
///
/// ```php
/// chr(65);  // "A"
/// chr(321); // "A", with a deprecation
/// ```
const CHR: FunctionEntry =
    FunctionEntry::sync("chr", &[Param::new("codepoint", ParamType::Int)], chr);

fn chr(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let codepoint = args::int(arguments, 0).unwrap_or_default();
    if !(0..=255).contains(&codepoint) {
        context.deprecated(
            "chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256"
                .into(),
        );
    }
    // Truncating to the low byte is the same as modulo 256, also for
    // negative numbers in two's complement.
    Ok(Value::from(vec![codepoint as u8]))
}

/// `utf8_encode("caf\xE9")` interprets the input as ISO-8859-1 regardless of locale.
const UTF8_ENCODE: FunctionEntry = FunctionEntry::deprecated_sync(
    "utf8_encode",
    &[Param::new("string", ParamType::String)],
    utf8_encode,
);

fn utf8_encode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    let non_ascii = bytes.iter().filter(|byte| !byte.is_ascii()).count();
    let mut output = buffer(bytes.len().saturating_add(non_ascii))?;
    for byte in bytes {
        let mut encoded = [0; 2];
        output.extend_from_slice(char::from(*byte).encode_utf8(&mut encoded).as_bytes());
    }
    Ok(Value::from(output))
}
