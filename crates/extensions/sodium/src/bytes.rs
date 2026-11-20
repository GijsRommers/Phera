//! `sodium_increment($bytes)` adds one in place, interpreting the string as little-endian.

use block_padding::{Iso7816, Padding};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};
use subtle::{Choice, ConditionallySelectable, ConstantTimeEq, ConstantTimeGreater};

use crate::{argument, exception, opaque_state::erase_string, zeroed};

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

fn compare(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let left = args::bytes(args, 0);
    let right = args::bytes(args, 1);
    same_length(left, right, "sodium_compare", "string1")?;
    let mut greater = Choice::from(0);
    let mut less = Choice::from(0);
    for (left, right) in left.iter().zip(right) {
        let equal = left.ct_eq(right);
        greater = (greater & equal) | left.ct_gt(right);
        less = (less & equal) | right.ct_gt(left);
    }
    Ok(Value::Int(
        i64::from(greater.unwrap_u8()) - i64::from(less.unwrap_u8()),
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
    if !matches!(*value, Value::String(_)) {
        return Err(exception("a PHP string is required"));
    }
    erase_string(&mut value);
    Ok(Value::Null)
}

fn increment(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let mut value = args::reference(args, 0)
        .ok_or_else(|| exception("a PHP string is required"))?
        .borrow_mut();
    let Value::String(string) = &mut *value else {
        return Err(exception("a PHP string is required"));
    };
    let mut carry = 1_u16;
    for byte in string.make_mut_bytes() {
        let sum = u16::from(*byte) + carry;
        *byte = sum as u8;
        carry = sum >> 8;
    }
    Ok(Value::Null)
}

fn add(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let mut value = args::reference(args, 0)
        .ok_or_else(|| exception("PHP strings are required"))?
        .borrow_mut();
    let Value::String(string) = &mut *value else {
        return Err(exception("PHP strings are required"));
    };
    let other = args::bytes(args, 1);
    same_length(string, other, "sodium_add", "string1")?;
    let mut carry = 0_u16;
    for (left, right) in string.make_mut_bytes().iter_mut().zip(other) {
        let sum = u16::from(*left) + u16::from(*right) + carry;
        *left = sum as u8;
        carry = sum >> 8;
    }
    Ok(Value::Null)
}

fn block_size(args: &[Value], function: &str) -> Result<usize, NativeError> {
    let size = args::int(args, 1).unwrap_or_default();
    if size <= 0 {
        return Err(argument(
            function,
            2,
            "block_size",
            "must be greater than 0",
        ));
    }
    usize::try_from(size).map_err(|_| argument(function, 2, "block_size", "is too large"))
}

fn pad(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let input = args::bytes(args, 0);
    let size = block_size(args, "sodium_pad")?;
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

fn unpad(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let input = args::bytes(args, 0);
    let size = block_size(args, "sodium_unpad")?;
    let start = input.len().checked_sub(size).ok_or_else(|| {
        argument(
            "sodium_unpad",
            1,
            "string",
            "must be at least as long as the block size",
        )
    })?;

    // Scan the entire last block. Bytes preceding the first marker are plaintext.
    let mut seen_marker = Choice::from(0);
    let mut invalid = Choice::from(0);
    let mut marker = 0_u64;
    for (offset, byte) in input[start..].iter().enumerate().rev() {
        let is_marker = byte.ct_eq(&0x80);
        marker.conditional_assign(&(offset as u64), !seen_marker & is_marker);
        invalid |= !seen_marker & !(byte.ct_eq(&0) | is_marker);
        seen_marker |= is_marker;
    }
    if !bool::from(seen_marker & !invalid) {
        return Err(exception("invalid padding"));
    }
    Ok(Value::from(&input[..start + marker as usize]))
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
            &[Param::new("string", ParamType::String)
                .raw_by_ref()
                .sensitive()],
            memzero,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_increment",
            &[Param::new("string", ParamType::String).raw_by_ref()],
            increment,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_add",
            &[
                Param::new("string1", ParamType::String).raw_by_ref(),
                Param::new("string2", ParamType::String),
            ],
            add,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "sodium_unpad",
            &[
                Param::new("string", ParamType::String).sensitive(),
                Param::new("block_size", ParamType::Int),
            ],
            unpad,
        ),
        "string",
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
