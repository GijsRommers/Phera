//! `unpack('nnumber/C2flag', $bytes)` reads three values into named array entries.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

use super::format::{Code, Endian, Number, count, diagnostic_byte, modifier};

const PARAMS: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("string", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
];

pub(super) const fn entry<const MODIFIERS: bool>() -> FunctionEntry {
    FunctionEntry::sync("unpack", PARAMS, |context, arguments| {
        unpack::<MODIFIERS>(context, arguments)
            .map_err(|error| context.native_call_error("unpack", arguments, error))
    })
}

fn unpack<const MODIFIERS: bool>(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (Some(format), Some(input)) = (args::string(arguments, 0), args::string(arguments, 1))
    else {
        return Err(NativeError::error(
            "unpack(): missing binary format or input",
        ));
    };
    let Some(data) = usize::try_from(args::int(arguments, 2).unwrap_or(0))
        .ok()
        .and_then(|offset| input.as_bytes().get(offset..))
    else {
        return Err(NativeError::value_error(
            "unpack(): Argument #3 ($offset) must be contained in argument #2 ($data)",
        ));
    };
    let mut position = 0usize;
    let mut output = PhpArray::new();
    let mut format = format.as_bytes();
    while let Some((&symbol, rest)) = format.split_first() {
        format = rest;
        let order = modifier::<MODIFIERS>(&mut format);
        let repeat = count(&mut format);
        if repeat.is_some_and(|repeat| repeat > i32::MAX as i64) {
            warning(context, symbol, "integer overflow");
            return Ok(Value::Bool(false));
        }
        let repeat = repeat.map(|repeat| repeat as usize);
        let name_end = format
            .iter()
            .position(|&byte| byte == b'/')
            .unwrap_or(format.len());
        let name = &format[..name_end.min(200)];
        format = format.get(name_end + 1..).unwrap_or_default();
        let code = Code::parse(symbol)
            .ok_or_else(|| {
                let symbol = diagnostic_byte(symbol);
                NativeError::value_error(format!(
                    "Invalid format type {}",
                    symbol.trim_end_matches('\0')
                ))
            })?
            .with_modifier(symbol, order)?;
        let remaining = &data[position..];
        match code {
            Code::Back => {
                let width = repeat.unwrap_or_else(|| {
                    warning(context, symbol, "'*' ignored");
                    1
                });
                position = position.saturating_sub(width);
            }
            Code::Seek => match repeat {
                Some(0) => (),
                Some(offset) if offset <= data.len() => position = offset,
                Some(_) => warning(context, symbol, "outside of string"),
                None => {
                    warning(context, symbol, "outside of string");
                    position = 0;
                }
            },
            Code::String(kind) | Code::Hex(kind) => {
                let width = repeat.map_or(remaining.len(), |count| match code {
                    Code::Hex(_) => count.div_ceil(2),
                    _ => count,
                });
                let Some(bytes) = checked_input(context, symbol, position, remaining, width) else {
                    return Ok(Value::Bool(false));
                };
                let value = if matches!(code, Code::Hex(_)) {
                    if repeat.is_some() && width > i32::MAX as usize / 2 {
                        return Err(NativeError::value_error(
                            "unpack(): Argument #1 ($format) repeater must be less than or equal \
                             to 1073741823",
                        ));
                    }
                    Value::from(read_hex(bytes, repeat, kind))
                } else {
                    Value::from(trimmed(bytes, kind))
                };
                output.insert(field_key(name, false, 1)?, value);
                position += width;
            }
            Code::Skip => {
                let width = repeat.unwrap_or(remaining.len());
                if width > remaining.len() {
                    let _ = checked_input(context, symbol, data.len(), &[], 1);
                    return Ok(Value::Bool(false));
                }
                position += width;
            }
            Code::Numeric(number, endian) => {
                let width = number.size();
                let repetitions = repeat.unwrap_or(remaining.len() / width);
                for index in 1..=repetitions {
                    let remaining = &data[position..];
                    let Some(bytes) = checked_input(context, symbol, position, remaining, width)
                    else {
                        return Ok(Value::Bool(false));
                    };
                    let key = field_key(name, repeat != Some(1), index)?;
                    output.insert(key, read_number(bytes, number, endian));
                    position += width;
                }
            }
        }
    }
    Ok(Value::from(output))
}

fn warning(context: &mut dyn Context, symbol: u8, suffix: &str) {
    context.warning(format!(
        "unpack(): Type {}: {suffix}",
        diagnostic_byte(symbol)
    ));
}

fn checked_input<'a>(
    context: &mut dyn Context,
    symbol: u8,
    position: usize,
    remaining: &'a [u8],
    width: usize,
) -> Option<&'a [u8]> {
    if width != 0 && position.saturating_add(width) > i32::MAX as usize + 1 {
        warning(context, symbol, "integer overflow");
        return None;
    }
    if let Some(bytes) = remaining.get(..width) {
        return Some(bytes);
    }
    let available = remaining.len();
    let verb = if available == 1 { "was" } else { "were" };
    warning(
        context,
        symbol,
        &format!(
            "not enough input values, need {width} values but only {available} {verb} provided"
        ),
    );
    None
}

fn field_key(name: &[u8], numbered: bool, index: usize) -> Result<ArrayKey, NativeError> {
    if name.is_empty() {
        return Ok(ArrayKey::Int(index as i64));
    }
    let mut key = name.to_vec();
    if numbered {
        key.extend_from_slice(index.to_string().as_bytes());
    }
    Ok(ArrayKey::from_value(&Value::from(key))?)
}

fn read_number(bytes: &[u8], number: Number, endian: Endian) -> Value {
    let mut wide = [0; 8];
    let size = bytes.len();
    let bits = if endian.little() {
        wide[..size].copy_from_slice(bytes);
        u64::from_le_bytes(wide)
    } else {
        wide[8 - size..].copy_from_slice(bytes);
        u64::from_be_bytes(wide)
    };
    match number {
        Number::Float => Value::Float(f32::from_bits(bits as u32) as f64),
        Number::Double => Value::Float(f64::from_bits(bits)),
        Number::Signed(size) => {
            let shift = (8 - size) * 8;
            Value::Int(((bits << shift) as i64) >> shift)
        }
        Number::Unsigned(_) => Value::Int(bits as i64),
    }
}

fn trimmed(bytes: &[u8], kind: u8) -> &[u8] {
    match kind {
        b'Z' => bytes.split(|&byte| byte == 0).next().unwrap_or_default(),
        b'A' => {
            let length = bytes
                .iter()
                .rposition(|&byte| !matches!(byte, b'\0' | b' ' | b'\t' | b'\r' | b'\n'))
                .map_or(0, |position| position + 1);
            &bytes[..length]
        }
        _ => bytes,
    }
}

fn read_hex(bytes: &[u8], repeat: Option<usize>, order: u8) -> Vec<u8> {
    let length = repeat.unwrap_or(bytes.len().saturating_mul(2));
    let digits = b"0123456789abcdef";
    let mut text = Vec::with_capacity(length);
    for &byte in bytes {
        let pair = if order == b'H' {
            [byte >> 4, byte & 15]
        } else {
            [byte & 15, byte >> 4]
        };
        text.push(digits[pair[0] as usize]);
        text.push(digits[pair[1] as usize]);
    }
    text.truncate(length);
    text
}
