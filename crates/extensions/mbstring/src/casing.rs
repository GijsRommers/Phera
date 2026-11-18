//! Changing case with Unicode's rules, which can change the length:
//!
//! ```php
//! mb_strtoupper('Straße');                      // "STRASSE"
//! mb_convert_case('hello world', MB_CASE_TITLE); // "Hello World"
//! mb_ucfirst('ǆemal');                         // "ǅemal": title case, not upper case
//! ```
//!
//! The mappings themselves are in [`crate::unicode`].

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::decode;
use crate::encoding::Encoding;
use crate::text::{decode_argument, encode_result, substring};
use crate::unicode::{self, Case};
use crate::{ENCODING, STRING};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_strtoupper",
        ONE_STRING,
        mb_strtoupper,
    ));
    registry.function(FunctionEntry::sync(
        "mb_strtolower",
        ONE_STRING,
        mb_strtolower,
    ));
    registry.function(FunctionEntry::sync("mb_ucfirst", ONE_STRING, mb_ucfirst));
    registry.function(FunctionEntry::sync("mb_lcfirst", ONE_STRING, mb_lcfirst));
    registry.function(FunctionEntry::sync(
        "mb_convert_case",
        MB_CONVERT_CASE,
        mb_convert_case,
    ));
}

const ONE_STRING: &[Param] = &[STRING, ENCODING];

/// `mb_strtoupper(string $string, ?string $encoding = null): string`
fn mb_strtoupper(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    change_case(ctx, a, "mb_strtoupper", Case::Upper)
}

/// `mb_strtolower(string $string, ?string $encoding = null): string`
fn mb_strtolower(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    change_case(ctx, a, "mb_strtolower", Case::Lower)
}

/// `mb_ucfirst(string $string, ?string $encoding = null): string`
fn mb_ucfirst(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    change_first(ctx, a, "mb_ucfirst", Case::Title)
}

/// `mb_lcfirst(string $string, ?string $encoding = null): string`
fn mb_lcfirst(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    change_first(ctx, a, "mb_lcfirst", Case::Lower)
}

const MB_CONVERT_CASE: &[Param] = &[STRING, Param::new("mode", ParamType::Int), ENCODING];

/// `mb_convert_case(string $string, int $mode, ?string $encoding = null): string`
fn mb_convert_case(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let case = Case::from_mode(args::int(a, 1).unwrap_or(0)).ok_or_else(|| {
        NativeError::value_error(
            "mb_convert_case(): Argument #2 ($mode) must be one of the MB_CASE_* constants",
        )
    })?;
    let (points, encoding) = decode_argument(ctx, a, 2, "mb_convert_case")?;
    Ok(Value::from(encode_result(
        ctx,
        &unicode::convert(&points, case),
        encoding,
    )))
}

fn change_case(ctx: &mut dyn Context, a: &[Value], function: &str, case: Case) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 1, function)?;
    if let Some(quirk) = uuencode_quirk(&points, encoding, args::bytes(a, 0)) {
        return Ok(quirk);
    }
    Ok(Value::from(encode_result(
        ctx,
        &unicode::convert(&points, case),
        encoding,
    )))
}

/// Changes the first character and copies the rest as it is, so invalid
/// bytes after it survive.
fn change_first(ctx: &mut dyn Context, a: &[Value], function: &str, case: Case) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 1, function)?;
    let bytes = args::bytes(a, 0);
    if let Some(quirk) = uuencode_quirk(&points, encoding, bytes) {
        return Ok(quirk);
    }
    if points.is_empty() {
        return Ok(Value::from(encode_result(ctx, &[], encoding)));
    }
    let first = substring(ctx, bytes, 0, Some(1), encoding);
    let mut changed = encode_result(
        ctx,
        &unicode::convert(&decode(&first, encoding), case),
        encoding,
    );
    if changed == *first {
        return Ok(args::value(a, 0).clone());
    }
    changed.extend_from_slice(&substring(ctx, bytes, 1, None, encoding));
    Ok(Value::from(changed))
}

/// When a non-empty string decodes to nothing as uuencode (`'x'`, or a
/// header without data), PHP returns a header and one space.
fn uuencode_quirk(points: &[u32], encoding: Encoding, bytes: &[u8]) -> Option<Value> {
    (encoding == Encoding::Uuencode && points.is_empty() && !bytes.is_empty())
        .then(|| Value::from("begin 0644 filename\n "))
}
