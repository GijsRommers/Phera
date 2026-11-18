//! `fscanf($file, '%d %s', $id, $name)` assigns fields from one input line.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use crate::handles::{STREAM, read_amount, read_notice};
use crate::resource::stream_arg;
use crate::scan_format::{Format, Item};
use crate::transfer::line;

pub fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        STREAM,
        Param::new("format", ParamType::String),
        Param::new("vars", ParamType::Mixed).by_ref().variadic(),
    ];
    registry.function(FunctionEntry::asynchronous("fscanf", PARAMS, scan));
}

fn scan(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "fscanf").map_err(|error| {
            if matches!(args::value(&arguments, 0), Value::Resource(_)) {
                NativeError::type_error(
                    "fscanf(): supplied resource is not a valid File-Handle resource",
                )
            } else {
                error
            }
        })?;
        let data = match line(resource, None).await? {
            Ok(data) if !data.is_empty() => data,
            Ok(_) => return Ok(Value::Bool(false)),
            Err(error) => {
                read_notice(context, "fscanf", &error, read_amount(None));
                return Ok(Value::Bool(false));
            }
        };
        let variables = arguments.len().saturating_sub(2);
        let format = Format::parse(args::bytes(&arguments, 1), variables)?;
        scan_line(&data, format, &arguments)
    })
}

fn scan_line(data: &[u8], format: Format<'_>, arguments: &[Value]) -> NativeResult {
    let data = data.split(|&b| b == 0).next().unwrap_or_default();
    let mut rest = data;
    let mut values = PhpArray::new();
    let with_references = arguments.len() > 2;
    if !with_references {
        for _ in 0..format.slots {
            values.push(Value::Null)?;
        }
    }
    let mut converted = 0;
    let mut underflow = false;
    for item in format.items {
        match item {
            Item::Space => rest = trim(rest),
            Item::Literal(byte) => {
                underflow = rest.is_empty();
                let Some(remainder) = rest.strip_prefix(&[byte]) else {
                    break;
                };
                rest = remainder;
            }
            Item::Field { field, width, slot } => {
                let (value, consumed) = if field == b"n" {
                    (Value::Int((data.len() - rest.len()) as i64), 0)
                } else {
                    if !matches!(field, b"c" | [b'[', ..]) {
                        rest = trim(rest);
                    }
                    if rest.is_empty() {
                        underflow = true;
                        break;
                    }
                    let input = &rest[..rest.len().min(width)];
                    let Some(found) = match_field(field, input) else {
                        break;
                    };
                    found
                };
                rest = &rest[consumed..];
                if let Some(index) = slot {
                    if with_references {
                        if let Some(target) = args::reference(arguments, index + 2) {
                            *target.borrow_mut() = value;
                        }
                    } else {
                        values.insert(phera_extension_api::ArrayKey::Int(index as i64), value);
                    }
                }
                converted += 1;
            }
        }
    }
    if converted == 0 && underflow {
        Ok(if with_references {
            Value::Int(-1)
        } else {
            Value::Null
        })
    } else if with_references {
        Ok(Value::Int(converted))
    } else {
        Ok(Value::from(values))
    }
}

fn trim(bytes: &[u8]) -> &[u8] {
    let consumed = bytes.iter().take_while(|&&b| whitespace(b)).count();
    &bytes[consumed..]
}

pub fn whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == 11
}

fn match_field(field: &[u8], input: &[u8]) -> Option<(Value, usize)> {
    match field {
        b"s" | b"c" => {
            let count = input.iter().take_while(|&&b| !whitespace(b)).count();
            Some((Value::from(input[..count].to_vec()), count))
        }
        [b'[', set @ ..] => {
            let count = input.iter().take_while(|&&b| contains(set, b)).count();
            (count != 0).then(|| (Value::from(input[..count].to_vec()), count))
        }
        b"f" | b"e" | b"E" | b"g" => float(input),
        [code] => integer(*code, input),
        _ => None,
    }
}

fn contains(set: &[u8], byte: u8) -> bool {
    let negated = set.first() == Some(&b'^');
    let mut set = if negated { &set[1..] } else { set };
    let mut matched = false;
    while let Some((&first, tail)) = set.split_first() {
        if let [b'-', last, rest @ ..] = tail {
            matched |= (first..=*last).contains(&byte);
            set = rest;
        } else {
            matched |= first == byte;
            set = tail;
        }
    }
    negated != matched
}

fn integer(code: u8, input: &[u8]) -> Option<(Value, usize)> {
    let (sign, digits) = match input {
        [first @ (b'+' | b'-'), rest @ ..] => (Some(*first), rest),
        _ => (None, input),
    };
    let detected = match (code, digits) {
        (b'i' | b'x' | b'X', [b'0', b'x' | b'X', digit, rest @ ..])
            if digit.is_ascii_hexdigit() =>
        {
            (16, &digits[digits.len() - rest.len() - 1..], 2)
        }
        (b'i', [b'0', ..]) | (b'o', _) => (8, digits, 0),
        (b'x' | b'X', _) => (16, digits, 0),
        _ => (10, digits, 0),
    };
    let (base, digits, prefix) = detected;
    let mut value = 0u64;
    let mut read = 0;
    for &digit in digits.iter().take(510) {
        let Some(digit) = char::from(digit).to_digit(base) else {
            break;
        };
        value = value
            .saturating_mul(u64::from(base))
            .saturating_add(u64::from(digit));
        read += 1;
    }
    if read == 0 {
        return None;
    }
    let number = if code == b'u' {
        let unsigned = if sign == Some(b'-') {
            value.wrapping_neg()
        } else {
            value
        };
        i64::try_from(unsigned).map_or_else(|_| Value::from(unsigned.to_string()), Value::Int)
    } else if sign == Some(b'-') {
        Value::Int(-(value.min(i64::MAX as u64 + 1) as i128) as i64)
    } else {
        Value::Int(value.min(i64::MAX as u64) as i64)
    };
    Some((number, usize::from(sign.is_some()) + prefix + read))
}

fn float(input: &[u8]) -> Option<(Value, usize)> {
    let mut cursor = usize::from(matches!(input.first(), Some(b'+' | b'-')));
    let before = digits_at(input, &mut cursor);
    let after = if input.get(cursor) == Some(&b'.') {
        cursor += 1;
        digits_at(input, &mut cursor)
    } else {
        0
    };
    if before + after == 0 {
        return None;
    }
    if matches!(input.get(cursor), Some(b'e' | b'E')) {
        let before_exponent = cursor;
        cursor += 1;
        cursor += usize::from(matches!(input.get(cursor), Some(b'+' | b'-')));
        if digits_at(input, &mut cursor) == 0 {
            cursor = before_exponent;
        }
    }
    let value = std::str::from_utf8(&input[..cursor])
        .ok()?
        .parse::<f64>()
        .ok()?;
    Some((Value::Float(value), cursor))
}

fn digits_at(input: &[u8], cursor: &mut usize) -> usize {
    let count = input[*cursor..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    *cursor += count;
    count
}
