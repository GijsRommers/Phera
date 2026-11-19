//! `sodium_increment($bytes)` adds one in place, interpreting the string as little-endian.

use block_padding::{Iso7816, Padding};
use crypto_bigint::{
    BoxedUint,
    ctutils::{CtGt, CtLt},
};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, Zeroizing};

use crate::{argument, exception, zeroed};

fn same_length(a: &[u8], b: &[u8], function: &str, name: &str) -> Result<(), NativeError> {
    if a.len() != b.len() {
        return Err(argument(
            function,
            1,
            name,
            "and argument #2 ($string_2) must have the same length",
        ));
    }
    Ok(())
}

fn number(bytes: &[u8]) -> Result<Zeroizing<BoxedUint>, NativeError> {
    let precision = u32::try_from(bytes.len())
        .ok()
        .and_then(|n| n.checked_mul(8))
        .ok_or_else(|| exception("arithmetic overflow"))?;
    Ok(Zeroizing::new(
        BoxedUint::from_le_slice(bytes, precision).map_err(|_| exception("arithmetic overflow"))?,
    ))
}

fn compare(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let left = args::bytes(args, 0);
    let right = args::bytes(args, 1);
    same_length(left, right, "sodium_compare", "string1")?;
    if left.is_empty() {
        return Ok(Value::Int(0));
    }
    let left = number(left)?;
    let right = number(right)?;
    Ok(Value::Int(
        i64::from(left.ct_gt(&right).to_u8()) - i64::from(left.ct_lt(&right).to_u8()),
    ))
}

fn memcmp(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let left = args::bytes(args, 0);
    let right = args::bytes(args, 1);
    same_length(left, right, "sodium_memcmp", "string1")?;
    Ok(Value::Int(i64::from(left.ct_eq(right).unwrap_u8()) - 1))
}

fn memzero(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let reference =
        args::reference(args, 0).ok_or_else(|| exception("a PHP string is required"))?;
    let mut value = reference.borrow_mut();
    let Value::String(string) = &mut *value else {
        return Err(exception("a PHP string is required"));
    };
    if let Some(bytes) = string.as_unique_bytes_mut() {
        bytes.zeroize();
    }
    *value = Value::Null;
    Ok(Value::Null)
}

fn increment(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    mutate_number(args, false)
}

fn add(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    mutate_number(args, true)
}

fn mutate_number(args: &[Value], add: bool) -> NativeResult {
    let error = if add {
        "PHP strings are required"
    } else {
        "a PHP string is required"
    };
    let reference = args::reference(args, 0).ok_or_else(|| exception(error))?;
    let mut value = reference.borrow_mut();
    let Value::String(string) = &*value else {
        return Err(exception(error));
    };
    let other = args::bytes(args, 1);
    if add {
        same_length(string, other, "sodium_add", "string1")?;
    }
    if string.is_empty() {
        return Ok(Value::Null);
    }
    let length = string.len();
    let mut integer = number(string)?;
    if add {
        integer.wrapping_add_assign(&*number(other)?);
    } else {
        integer.wrapping_add_assign(BoxedUint::one());
    }
    let bytes = Zeroizing::new(integer.to_le_bytes());
    *value = Value::from(&bytes[..length]);
    Ok(Value::Null)
}

fn pad(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let input = args::bytes(args, 0);
    let size = args::int(args, 1).unwrap_or_default();
    if size <= 0 {
        return Err(argument(
            "sodium_pad",
            2,
            "block_size",
            "must be greater than 0",
        ));
    }
    let size = usize::try_from(size)
        .map_err(|_| argument("sodium_pad", 2, "block_size", "is too large"))?;
    let remainder = input.len() % size;
    let length = input
        .len()
        .checked_add(size - remainder)
        .ok_or_else(|| exception("input is too large"))?;
    let mut output = zeroed(length)?;
    output[..input.len()].copy_from_slice(input);
    Iso7816::raw_pad(&mut output[length - size..], remainder);
    Ok(Value::from(output))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_compare",
            &[
                Param::new("string1", ParamType::String).sensitive(),
                Param::new("string2", ParamType::String).sensitive(),
            ],
            compare,
        ),
        "int",
    ),
    (
        FunctionEntry::sync(
            "sodium_memcmp",
            &[
                Param::new("string1", ParamType::String).sensitive(),
                Param::new("string2", ParamType::String).sensitive(),
            ],
            memcmp,
        ),
        "int",
    ),
    (
        FunctionEntry::sync(
            "sodium_memzero",
            &[Param::new("string", ParamType::Mixed).by_ref().sensitive()],
            memzero,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_increment",
            &[Param::new("string", ParamType::Mixed).by_ref()],
            increment,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_add",
            &[
                Param::new("string1", ParamType::Mixed).by_ref(),
                Param::new("string2", ParamType::String),
            ],
            add,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_pad",
            &[
                Param::new("string", ParamType::String).sensitive(),
                Param::new("block_size", ParamType::Int),
            ],
            pad,
        ),
        "string",
    ),
];
