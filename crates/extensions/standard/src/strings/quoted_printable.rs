//! `quoted_printable_encode("café")` encodes non-ASCII mail body bytes.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

pub(super) const ENCODE: FunctionEntry = FunctionEntry::sync(
    "quoted_printable_encode",
    &[Param::new("string", ParamType::String)],
    encode,
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
