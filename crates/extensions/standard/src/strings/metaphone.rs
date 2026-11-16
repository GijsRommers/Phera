//! Traditional PHP metaphone, including NUL termination and phoneme-limit quirks.
use super::{buffer, bytes};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};
const PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("max_phonemes", ParamType::Int).optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("metaphone", PARAMS, metaphone));
}
fn vowel(byte: u8) -> bool {
    b"AEIOU".contains(&byte)
}
fn soft(byte: u8) -> bool {
    b"EIY".contains(&byte)
}
/// PHP may emit two phonemes for X even when only one remains before the limit.
fn metaphone(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let limit = args::int(arguments, 1).unwrap_or_default();
    if limit < 0 {
        return Err(NativeError::value_error(
            "metaphone(): Argument #2 ($max_phonemes) must be greater than or equal to 0",
        ));
    }
    let input = args::bytes(arguments, 0);
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(input.len());
    let input = input.get(..end).unwrap_or_default();
    let mut result = buffer(input.len().saturating_mul(2))?;
    let letter = |index: usize| {
        input
            .get(index)
            .copied()
            .unwrap_or_default()
            .to_ascii_uppercase()
    };
    let Some(mut cursor) = input.iter().position(|&byte| super::alphabetic(byte)) else {
        return Ok(bytes(result));
    };
    let current = letter(cursor);
    let next = letter(cursor + 1);
    match current {
        b'A' => {
            result.push(if next == b'E' { b'E' } else { b'A' });
            cursor += if next == b'E' { 2 } else { 1 };
        }
        b'G' | b'K' | b'P' if next == b'N' => {
            result.push(b'N');
            cursor += 2;
        }
        b'W' if next == b'R' || next == b'H' || vowel(next) => {
            result.push(if next == b'R' { b'R' } else { b'W' });
            cursor += 2;
        }
        b'X' => {
            result.push(b'S');
            cursor += 1;
        }
        b'E' | b'I' | b'O' | b'U' => {
            result.push(current);
            cursor += 1;
        }
        _ => {}
    }
    while cursor < input.len() && (limit == 0 || result.len() < limit as usize) {
        let current = letter(cursor);
        let previous = cursor.checked_sub(1).map_or(0, letter);
        let next = letter(cursor + 1);
        let after = letter(cursor + 2);
        let mut skip = 0;
        if current.is_ascii_alphabetic() && (current != previous || current == b'C') {
            let phoneme = match current {
                b'B' if previous != b'M' => Some(b'B'),
                b'C' => {
                    if soft(next) {
                        if next == b'I' && after == b'A' {
                            Some(b'X')
                        } else if previous == b'S' {
                            None
                        } else {
                            Some(b'S')
                        }
                    } else if next == b'H' {
                        skip = 1;
                        Some(b'X')
                    } else {
                        Some(b'K')
                    }
                }
                b'D' => {
                    if next == b'G' && soft(after) {
                        skip = 1;
                        Some(b'J')
                    } else {
                        Some(b'T')
                    }
                }
                b'G' => {
                    if next == b'H' {
                        let back = cursor.checked_sub(3).map_or(0, letter);
                        let back4 = cursor.checked_sub(4).map_or(0, letter);
                        if !b"BDH".contains(&back) && back4 != b'H' {
                            skip = 1;
                            Some(b'F')
                        } else {
                            None
                        }
                    } else if next == b'N' {
                        if !super::alphabetic(after) || after == b'E' && letter(cursor + 3) == b'D'
                        {
                            None
                        } else {
                            Some(b'K')
                        }
                    } else if soft(next) && previous != b'G' {
                        Some(b'J')
                    } else {
                        Some(b'K')
                    }
                }
                b'H' if vowel(next) && !b"CGPST".contains(&previous) => Some(b'H'),
                b'K' if previous != b'C' => Some(b'K'),
                b'P' => Some(if next == b'H' { b'F' } else { b'P' }),
                b'Q' => Some(b'K'),
                b'S' => {
                    if next == b'I' && matches!(after, b'O' | b'A') {
                        Some(b'X')
                    } else if next == b'H' {
                        skip = 1;
                        Some(b'X')
                    } else {
                        Some(b'S')
                    }
                }
                b'T' => {
                    if next == b'I' && matches!(after, b'O' | b'A') {
                        Some(b'X')
                    } else if next == b'H' {
                        skip = 1;
                        Some(b'0')
                    } else if next == b'C' && after == b'H' {
                        None
                    } else {
                        Some(b'T')
                    }
                }
                b'V' => Some(b'F'),
                b'W' | b'Y' if vowel(next) => Some(current),
                b'X' => {
                    result.push(b'K');
                    Some(b'S')
                }
                b'Z' => Some(b'S'),
                b'F' | b'J' | b'L' | b'M' | b'N' | b'R' => Some(current),
                _ => None,
            };
            if let Some(phoneme) = phoneme {
                result.push(phoneme);
            }
        }
        cursor += 1 + skip;
    }
    Ok(bytes(result))
}
