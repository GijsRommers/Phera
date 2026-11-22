//! `convert_uuencode("A")` packages binary data into lines of printable bytes.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::sync("convert_uuencode", PARAMS, encode),
    FunctionEntry::sync("convert_uudecode", PARAMS, decode),
];

const PARAMS: &[Param] = &[Param::new("string", ParamType::String)];

fn encode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let mut output = super::buffer(
        input
            .len()
            .div_ceil(3)
            .saturating_mul(4)
            .saturating_add(input.len().div_ceil(45).saturating_mul(2))
            .saturating_add(2),
    )?;
    for line in input.chunks(45) {
        output.push(printable(line.len() as u8));
        for bytes in line.chunks(3) {
            let a = bytes[0];
            let b = bytes.get(1).copied().unwrap_or_default();
            let c = bytes.get(2).copied().unwrap_or_default();
            output.extend([
                printable(a >> 2),
                printable((a << 4 | b >> 4) & 63),
                printable((b << 2 | c >> 6) & 63),
                printable(c & 63),
            ]);
        }
        output.push(b'\n');
    }
    output.extend_from_slice(b"`\n");
    Ok(Value::from(output))
}

fn printable(byte: u8) -> u8 {
    if byte == 0 { b'`' } else { byte + b' ' }
}

fn decode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    if input.is_empty() {
        return invalid(context);
    }
    let mut output = super::buffer(input.len())?;
    let input = input.strip_suffix(b"\n").unwrap_or(input);
    for line in input.split(|&byte| byte == b'\n') {
        let Some((&count, bytes)) = line.split_first() else {
            return invalid(context);
        };
        let count = usize::from(six_bits(count));
        if count == 0 {
            break;
        }
        let width = count.div_ceil(3) * 4;
        let Some(data) = bytes.get(..width) else {
            return invalid(context);
        };
        let end = output.len() + count;
        for &[a, b, c, d] in data.as_chunks::<4>().0 {
            let (a, b, c, d) = (six_bits(a), six_bits(b), six_bits(c), six_bits(d));
            output.extend([a << 2 | b >> 4, b << 4 | c >> 2, c << 6 | d]);
        }
        output.truncate(end);
    }
    Ok(Value::from(output))
}

fn six_bits(byte: u8) -> u8 {
    byte.wrapping_sub(b' ') & 63
}

fn invalid(context: &mut dyn Context) -> NativeResult {
    context
        .warning("convert_uudecode(): Argument #1 ($data) is not a valid uuencoded string".into());
    Ok(Value::Bool(false))
}
