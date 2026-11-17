//! timelib's format grammar: `date_parse_from_format('!Y-m-d', '2024-02-30')`
//! retains February 30 and reports its warning instead of rejecting the parse.

mod numeric;
mod result;
mod text;
mod zone;

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};
use result::Parsed;

pub fn function(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 1);
    if input.contains(&0) {
        return Err(NativeError::value_error(
            "date_parse_from_format(): Argument #2 ($datetime) must not contain any null bytes",
        ));
    }
    Ok(parse(args::bytes(arguments, 0), input).array())
}

pub fn parse(format: &[u8], input: &[u8]) -> Parsed {
    let format = format.split(|byte| *byte == 0).next().unwrap_or_default();
    let mut parsed = Parsed::default();
    let mut cursor = 0;
    let mut position = 0;
    let mut extra = false;
    while cursor < format.len() && position < input.len() {
        let token = format[cursor];
        let start = position;
        if numeric::parse(token, input, &mut position, &mut parsed)
            || text::parse(token, input, &mut position, &mut parsed)
        {
            cursor += 1;
            continue;
        }
        match token {
            b'!' => parsed.reset(false),
            b'|' => parsed.reset(true),
            b'+' => extra = true,
            b'?' => position += 1,
            b'*' => {
                position += 1;
                while input
                    .get(position)
                    .is_some_and(|byte| !b" \t.,:;/-0123456789".contains(byte))
                {
                    position += 1;
                }
            }
            b'#' => {
                if b";:/.,-()".contains(&input[position]) {
                    position += 1;
                } else {
                    parsed.error(start, "The separation symbol ([;:/.,-]) could not be found");
                }
            }
            b';' | b':' | b'/' | b'.' | b',' | b'-' | b'(' | b')' => {
                if input[position] == token {
                    position += 1;
                } else {
                    parsed.error(start, "The separation symbol could not be found");
                }
            }
            b'\\' => {
                if let Some(literal) = format.get(cursor + 1) {
                    cursor += 1;
                    if input[position] == *literal {
                        position += 1;
                    } else {
                        parsed.error(start, "The escaped character could not be found");
                    }
                } else {
                    parsed.error(start, "Escaped character expected");
                }
            }
            b'e' | b'P' | b'p' | b'T' | b'O' => {
                let (zone, found) = zone::parse(input, &mut position);
                parsed.zone = Some(zone);
                if !found {
                    parsed.error(start, "The timezone could not be found in the database");
                }
            }
            _ => {
                if input[position] != token {
                    parsed.error(start, "The format separator does not match");
                }
                position += 1;
            }
        }
        cursor += 1;
    }
    if position < input.len() {
        if extra {
            parsed.warnings.push((position, "Trailing data".into()));
        } else {
            parsed.error(position, "Trailing data");
        }
    }
    while let Some(token) = format.get(cursor) {
        match token {
            b'!' => parsed.reset(false),
            b'|' => parsed.reset(true),
            b'+' => {}
            _ => {
                parsed.error(position, "Not enough data available to satisfy format");
                break;
            }
        }
        cursor += 1;
    }
    parsed.finish(position);
    parsed
}
