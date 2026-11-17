//! Numeric HTML entities for the code points in a map of ranges. The map is
//! a flat list of four integers per range: first, last, an offset added to
//! the code point and a mask applied after it.
//!
//! ```php
//! $map = [0x80, 0x10FFFF, 0, 0x1FFFFF];
//! mb_encode_numericentity('aé', $map);        // "a&#233;"
//! mb_encode_numericentity('aé', $map, null, true); // "a&#xE9;"
//! mb_decode_numericentity('a&#233;', $map);   // "aé"
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::text::{decode_argument, encode_result};
use crate::{ENCODING, STRING};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_encode_numericentity",
        MB_ENCODE_NUMERICENTITY,
        mb_encode_numericentity,
    ));
    registry.function(FunctionEntry::sync(
        "mb_decode_numericentity",
        MB_DECODE_NUMERICENTITY,
        mb_decode_numericentity,
    ));
}

const MAP: Param = Param::new("map", ParamType::Array);

const MB_ENCODE_NUMERICENTITY: &[Param] = &[
    STRING,
    MAP,
    ENCODING,
    Param::new("hex", ParamType::Bool).optional(),
];

/// `mb_encode_numericentity(string $string, array $map, ?string $encoding = null,
/// bool $hex = false): string`
fn mb_encode_numericentity(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let function = "mb_encode_numericentity";
    let ranges = ranges(a, function)?;
    let (points, encoding) = decode_argument(ctx, a, 2, function)?;
    let hex = args::bool(a, 3).unwrap_or(false);
    let mut output = Vec::with_capacity(points.len());
    for point in points {
        let Some(range) = ranges
            .iter()
            .find(|range| (range[0]..=range[1]).contains(&point))
        else {
            output.push(point);
            continue;
        };
        let number = point.wrapping_add(range[2]) & range[3];
        let entity = if hex {
            format!("&#x{number:X};")
        } else {
            format!("&#{number};")
        };
        output.extend(entity.bytes().map(u32::from));
    }
    Ok(Value::from(encode_result(ctx, &output, encoding)))
}

const MB_DECODE_NUMERICENTITY: &[Param] = &[STRING, MAP, ENCODING];

/// `mb_decode_numericentity(string $string, array $map, ?string $encoding = null): string`:
/// an entity is decoded when the number minus a range's offset falls in it.
fn mb_decode_numericentity(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let function = "mb_decode_numericentity";
    let ranges = ranges(a, function)?;
    let (points, encoding) = decode_argument(ctx, a, 2, function)?;
    let mut output = Vec::with_capacity(points.len());
    let mut index = 0;
    while index < points.len() {
        if let Some((number, end)) = entity(&points, index)
            && let Some(original) = ranges.iter().find_map(|range| {
                let original = number.wrapping_sub(range[2]);
                (range[0]..=range[1])
                    .contains(&original)
                    .then_some(original)
            })
        {
            output.push(original);
            index = end;
            continue;
        }
        output.push(points[index]);
        index += 1;
    }
    Ok(Value::from(encode_result(ctx, &output, encoding)))
}

/// The map as ranges of four. PHP converts each element to an integer and
/// keeps its low 32 bits.
fn ranges(a: &[Value], function: &str) -> Result<Vec<[u32; 4]>, NativeError> {
    let map = args::array(a, 1).cloned().unwrap_or_default();
    if map.len() % 4 != 0 {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($map) must have a multiple of 4 elements"
        )));
    }
    let numbers: Vec<u32> = map
        .iter()
        .map(|(_, value)| match value {
            Value::Int(number) => *number as u32,
            Value::Float(number) => *number as u32,
            Value::Bool(flag) => u32::from(*flag),
            other => crate::string_of(other).parse().unwrap_or(0),
        })
        .collect();
    Ok(numbers.as_chunks::<4>().0.to_vec())
}

/// `&#` followed by decimal digits, or `&#x` and hexadecimal ones, with an
/// optional `;`: the number and where the entity ends.
fn entity(points: &[u32], start: usize) -> Option<(u32, usize)> {
    if points.get(start..start + 2)? != [u32::from(b'&'), u32::from(b'#')] {
        return None;
    }
    let mut index = start + 2;
    let radix = if matches!(points.get(index), Some(&point) if point == u32::from(b'x') || point == u32::from(b'X'))
    {
        index += 1;
        16
    } else {
        10
    };
    let first = index;
    let mut number = 0u32;
    while let Some(&point) = points.get(index) {
        let Some(digit) = char::from_u32(point)?.to_digit(radix) else {
            break;
        };
        number = number.checked_mul(radix)?.checked_add(digit)?;
        index += 1;
    }
    if index == first {
        return None;
    }
    if points.get(index) == Some(&u32::from(b';')) {
        index += 1;
    }
    Some((number, index))
}
