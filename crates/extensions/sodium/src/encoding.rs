//! Binary encodings such as `sodium_hex2bin('00:ff', ':')` preserve arbitrary bytes.

use base64::{Engine, engine::general_purpose};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::argument;

const BIN2HEX: &str = "sodium_bin2hex";
const HEX2BIN: &str = "sodium_hex2bin";
const BIN2BASE64: &str = "sodium_bin2base64";
const BASE642BIN: &str = "sodium_base642bin";

const STRING: Param = Param::new("string", ParamType::String).sensitive();
const IGNORE: Param =
    Param::new("ignore", ParamType::String).with_default(ConstantValue::String(""));
const ID: Param = Param::new("id", ParamType::Int);

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (FunctionEntry::sync(BIN2HEX, &[STRING], bin2hex), "string"),
    (
        FunctionEntry::sync(HEX2BIN, &[STRING, IGNORE], hex2bin),
        "string",
    ),
    (
        FunctionEntry::sync(BIN2BASE64, &[STRING, ID], bin2base64),
        "string",
    ),
    (
        FunctionEntry::sync(BASE642BIN, &[STRING, ID, IGNORE], base642bin),
        "string",
    ),
];

pub const CONSTANTS: &[(&str, i64)] = &[
    ("SODIUM_BASE64_VARIANT_ORIGINAL", 1),
    ("SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING", 3),
    ("SODIUM_BASE64_VARIANT_URLSAFE", 5),
    ("SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING", 7),
];

fn ignore_set(bytes: &[u8]) -> &[u8] {
    bytes.split(|&byte| byte == 0).next().unwrap_or_default()
}

fn bin2hex(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::from(hex::encode(args::bytes(arguments, 0))))
}

fn hex2bin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let ignore = ignore_set(args::bytes(arguments, 1));
    let mut digits = Vec::with_capacity(input.len());
    for &byte in input {
        if byte.is_ascii_hexdigit() || digits.len() % 2 != 0 || !ignore.contains(&byte) {
            digits.push(byte);
        }
    }
    hex::decode(digits)
        .map(Value::from)
        .map_err(|_| argument(HEX2BIN, 1, "string", "must be a valid hexadecimal string"))
}

fn variant(
    arguments: &[Value],
    function: &str,
) -> Result<&'static general_purpose::GeneralPurpose, NativeError> {
    match args::int(arguments, 1).unwrap_or_default() as u32 {
        1 => Ok(&general_purpose::STANDARD),
        3 => Ok(&general_purpose::STANDARD_NO_PAD),
        5 => Ok(&general_purpose::URL_SAFE),
        7 => Ok(&general_purpose::URL_SAFE_NO_PAD),
        _ => Err(argument(
            function,
            2,
            "id",
            "must be a valid base64 variant identifier",
        )),
    }
}

fn bin2base64(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let codec = variant(arguments, BIN2BASE64)?;
    Ok(Value::from(codec.encode(args::bytes(arguments, 0))))
}

fn base642bin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let codec = variant(arguments, BASE642BIN)?;
    let ignore = ignore_set(args::bytes(arguments, 2));
    let url_safe = matches!(args::int(arguments, 1).unwrap_or_default() as u32, 5 | 7);
    let input = args::bytes(arguments, 0);
    let filtered: Vec<u8> = input
        .iter()
        .copied()
        .filter(|&byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'='
                || if url_safe {
                    matches!(byte, b'-' | b'_')
                } else {
                    matches!(byte, b'+' | b'/')
                }
                || !ignore.contains(&byte)
        })
        .collect();
    codec
        .decode(filtered)
        .map(Value::from)
        .map_err(|_| argument(BASE642BIN, 1, "string", "must be a valid base64 string"))
}
