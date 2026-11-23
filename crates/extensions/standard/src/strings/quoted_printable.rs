//! `quoted_printable_encode("café")` encodes non-ASCII mail body bytes.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

pub(super) const ENCODE: FunctionEntry = FunctionEntry::sync(
    "quoted_printable_encode",
    &[Param::new("string", ParamType::String)],
    encode,
);

pub(super) const DECODE: FunctionEntry = FunctionEntry::sync(
    "quoted_printable_decode",
    &[Param::new("string", ParamType::String)],
    decode,
);

fn encode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let mut output = Vec::with_capacity(input.len());
    let mut column = 0;
    let mut bytes = input;
    while let Some((&byte, rest)) = bytes.split_first() {
        if byte == b'\r'
            && let Some(after) = rest.strip_prefix(b"\n")
        {
            output.extend_from_slice(b"\r\n");
            column = 0;
            bytes = after;
            continue;
        }
        let encode = !(32..=126).contains(&byte)
            || byte == b'='
            || byte == b' ' && rest.starts_with(b"\r\n");
        let width = if encode { 3 } else { 1 };
        if column + width > 75 {
            output.extend_from_slice(b"=\r\n");
            column = 0;
        }
        column += width;
        if encode {
            const HEX: &[u8] = b"0123456789ABCDEF";
            output.extend([
                b'=',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 15)],
            ]);
        } else {
            output.push(byte);
        }
        bytes = rest;
    }
    Ok(Value::from(output))
}

fn decode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let mut output = super::buffer(input.len())?;
    let mut remaining = input;
    while let Some((&byte, rest)) = remaining.split_first() {
        if byte == b'=' {
            if let [high, low, after @ ..] = rest
                && let (Some(high), Some(low)) = (hex(*high), hex(*low))
            {
                output.push(high << 4 | low);
                remaining = after;
                continue;
            }
            let mut end = rest;
            while let Some((b' ' | b'\t', after)) = end.split_first() {
                end = after;
            }
            if let Some(after) = end
                .strip_prefix(b"\r\n")
                .or_else(|| end.strip_prefix(b"\r"))
                .or_else(|| end.strip_prefix(b"\n"))
            {
                remaining = after;
                continue;
            }
            if end.is_empty() {
                break;
            }
        }
        output.push(byte);
        remaining = rest;
    }
    Ok(Value::from(output))
}

fn hex(byte: u8) -> Option<u8> {
    (byte as char).to_digit(16).map(|value| value as u8)
}
