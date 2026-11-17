//! Numeric entities use four-integer ranges: `mb_encode_numericentity('é', [128,65535,0,65535])`.
use crate::{encoding_name, text};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

pub fn numeric(ctx: &mut dyn Context, a: &[Value], decoding: bool) -> NativeResult {
    let name = if decoding {
        "mb_decode_numericentity"
    } else {
        "mb_encode_numericentity"
    };
    let array = args::array(a, 1).cloned().unwrap_or_default();
    if array.len() % 4 != 0 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #2 ($map) must have a multiple of 4 elements"
        )));
    }
    let map: Vec<u32> = array
        .iter()
        .map(|(_, value)| match value {
            Value::Int(n) => *n as u32,
            Value::Float(n) => *n as u32,
            Value::Bool(v) => u32::from(*v),
            _ => encoding_name(value).parse().unwrap_or(0),
        })
        .collect();
    let (points, encoding) = text::decode_argument(ctx, a, 2, name)?;
    let mut output = Vec::new();
    let mut i = 0;
    while i < points.len() {
        if decoding {
            if let Some((cp, end)) = entity(&points, i) {
                let replacement = map.as_chunks::<4>().0.iter().find_map(|range| {
                    let original = cp.wrapping_sub(range[2]);
                    (original >= range[0] && original <= range[1]).then_some(original)
                });
                if let Some(cp) = replacement {
                    output.push(cp);
                    i = end;
                    continue;
                }
            }
            output.push(points[i]);
        } else {
            let cp = points[i];
            if let Some(range) = map
                .as_chunks::<4>()
                .0
                .iter()
                .find(|r| cp >= r[0] && cp <= r[1])
            {
                let number = cp.wrapping_add(range[2]) & range[3];
                let entity = if args::bool(a, 3).unwrap_or(false) {
                    format!("&#x{number:X};")
                } else {
                    format!("&#{number};")
                };
                output.extend(entity.bytes().map(u32::from));
            } else {
                output.push(cp);
            }
        }
        i += 1;
    }
    Ok(Value::from(text::encode_result(ctx, &output, encoding)))
}
fn entity(points: &[u32], start: usize) -> Option<(u32, usize)> {
    if points.get(start..start + 2)? != [38, 35] {
        return None;
    }
    let mut i = start + 2;
    let radix = if matches!(points.get(i), Some(120 | 88)) {
        i += 1;
        16
    } else {
        10
    };
    let first = i;
    let mut number = 0u32;
    while let Some(cp) = points.get(i) {
        let Some(digit) = char::from_u32(*cp)?.to_digit(radix) else {
            break;
        };
        number = number.checked_mul(radix)?.checked_add(digit)?;
        i += 1;
    }
    if i == first {
        return None;
    }
    if points.get(i) == Some(&59) {
        i += 1;
    }
    Some((number, i))
}
