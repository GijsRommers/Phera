//! `pack('a3H3X1C', 'z', 'abc', 4)` writes binary strings and can rewind the cursor.

use phera_extension_api::value::{to_float, to_int};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value,
};

use super::format::{Code, Number, count, diagnostic_byte};

pub const PACK: FunctionEntry = FunctionEntry::asynchronous(
    "pack",
    &[
        Param::new("format", ParamType::String),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    |context, mut arguments| {
        Box::pin(async move {
            let result = pack(context, &mut arguments).await;
            result.map_err(|error| context.native_call_error("pack", &arguments, error))
        })
    },
);

struct Field {
    symbol: u8,
    code: Code,
    count: usize,
}

impl Field {
    fn message(&self, suffix: &str) -> String {
        type_message(self.symbol, suffix)
    }
}

fn type_message(symbol: u8, suffix: &str) -> String {
    if symbol == 0 {
        return "Type ".into();
    }
    format!("Type {}: {suffix}", diagnostic_byte(symbol))
}

async fn pack(context: &mut dyn Context, args: &mut [Value]) -> NativeResult {
    let Some((first, values)) = args.split_first_mut() else {
        return Err(NativeError::error("pack(): missing binary format"));
    };
    let Some(format) = first.as_str_bytes() else {
        return Err(NativeError::error("pack(): missing binary format"));
    };
    let fields = parse(context, format, values).await?;
    let size = output_size(context, &fields)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(size)
        .map_err(|_| NativeError::value_error("pack(): output size exceeds available memory"))?;
    output.resize(size, 0);
    let mut position = 0usize;
    let mut values = values.iter();
    for field in fields {
        match field.code {
            Code::Back => position = position.saturating_sub(field.count),
            Code::Seek => {
                if field.count > position {
                    output[position..field.count].fill(0);
                }
                position = field.count;
            }
            Code::Skip => {
                output[position..position + field.count].fill(0);
                position += field.count;
            }
            Code::String(kind) => {
                let value = values.next().ok_or_else(missing_value)?;
                let text = context.stringify(value).await?;
                let room = field.count.saturating_sub(usize::from(kind == b'Z'));
                let size = room.min(text.len());
                let dest = &mut output[position..position + field.count];
                dest.fill(if kind == b'A' { b' ' } else { 0 });
                dest[..size].copy_from_slice(&text.as_bytes()[..size]);
                position += field.count;
            }
            Code::Hex(order) => {
                let value = values.next().ok_or_else(missing_value)?;
                let text = context.stringify(value).await?;
                let size = field.count.min(text.len());
                if size < field.count {
                    context.warning(format!(
                        "pack(): {}",
                        field.message("not enough characters in string")
                    ));
                }
                for (index, &digit) in text.as_bytes()[..size].iter().enumerate() {
                    let nibble = (digit as char).to_digit(16).unwrap_or_else(|| {
                        context.warning(format!(
                            "pack(): Type {}: illegal hex digit {}",
                            order as char,
                            diagnostic_byte(digit)
                        ));
                        0
                    }) as u8;
                    let byte = &mut output[position + index / 2];
                    if index % 2 == 0 {
                        *byte = 0;
                    }
                    if (index % 2 == 0) == (order == b'H') {
                        *byte |= nibble << 4;
                    } else {
                        *byte |= nibble;
                    }
                }
                position += size.div_ceil(2);
            }
            Code::Numeric(number, endian) => {
                let width = number.size();
                for _ in 0..field.count {
                    let value = values.next().ok_or_else(missing_value)?;
                    let bits = match number {
                        Number::Float => {
                            (to_float(value, context.diagnostics())? as f32).to_bits() as u64
                        }
                        Number::Double => to_float(value, context.diagnostics())?.to_bits(),
                        _ => to_int(value, context.diagnostics())? as u64,
                    };
                    let dest = &mut output[position..position + width];
                    if endian.little() {
                        dest.copy_from_slice(&bits.to_le_bytes()[..width]);
                    } else {
                        dest.copy_from_slice(&bits.to_be_bytes()[8 - width..]);
                    }
                    position += width;
                }
            }
        }
    }
    output.truncate(position);
    Ok(Value::from(output))
}

fn missing_value() -> NativeError {
    NativeError::error("pack(): missing planned argument")
}

async fn parse(
    context: &mut dyn Context,
    mut format: &[u8],
    values: &mut [Value],
) -> Result<Vec<Field>, NativeError> {
    let mut fields = Vec::new();
    let mut cursor = 0usize;
    while let Some((&symbol, rest)) = format.split_first() {
        format = rest;
        let repeat = count(&mut format).and_then(|n| usize::try_from(n as i32).ok());
        let code = Code::parse(symbol)
            .ok_or_else(|| NativeError::value_error(type_message(symbol, "unknown format code")))?;
        let count = match code {
            Code::String(kind) | Code::Hex(kind) => {
                let value = values.get_mut(cursor).ok_or_else(|| {
                    NativeError::value_error(type_message(symbol, "not enough arguments"))
                })?;
                cursor += 1;
                if let Some(repeat) = repeat {
                    repeat
                } else {
                    let string = context.stringify(value).await?;
                    let length = string.len() + usize::from(kind == b'Z');
                    *value = Value::String(string);
                    length
                }
            }
            Code::Numeric(_, _) => {
                let remaining = values.len().saturating_sub(cursor);
                let repeat = repeat.unwrap_or(remaining);
                if repeat > remaining {
                    return Err(NativeError::value_error(type_message(
                        symbol,
                        "too few arguments",
                    )));
                }
                cursor += repeat;
                repeat
            }
            Code::Skip | Code::Back | Code::Seek => repeat.unwrap_or_else(|| {
                context.warning(format!("pack(): {}", type_message(symbol, "'*' ignored")));
                1
            }),
        };
        fields.push(Field {
            symbol,
            code,
            count,
        });
    }
    if cursor < values.len() {
        context.warning(format!(
            "pack(): {} arguments unused",
            values.len() - cursor
        ));
    }
    Ok(fields)
}

fn output_size(context: &mut dyn Context, fields: &[Field]) -> Result<usize, NativeError> {
    let mut position = 0usize;
    let mut longest = 0usize;
    for field in fields {
        let add = match field.code {
            Code::Back => {
                if field.count > position {
                    context.warning("pack(): Type X: outside of string".into());
                }
                position = position.saturating_sub(field.count);
                0
            }
            Code::Seek => {
                position = field.count;
                0
            }
            Code::Hex(_) => field.count.div_ceil(2),
            Code::Numeric(number, _) => field.count.saturating_mul(number.size()),
            _ => field.count,
        };
        position = position.saturating_add(add);
        if position > i32::MAX as usize {
            return Err(NativeError::value_error(
                field.message("integer overflow in format string"),
            ));
        }
        longest = longest.max(position);
    }
    Ok(longest)
}
