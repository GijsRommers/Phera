//! `fscanf($file, '%2$d %1$s')` places the integer in the second slot.

use std::collections::HashSet;

use phera_extension_api::NativeError;

use crate::scan::whitespace;

pub enum Item<'a> {
    Space,
    Literal(u8),
    Field {
        field: &'a [u8],
        width: usize,
        slot: Option<usize>,
    },
}

pub struct Format<'a> {
    pub items: Vec<Item<'a>>,
    pub slots: usize,
}

impl<'a> Format<'a> {
    pub fn parse(format: &'a [u8], variables: usize) -> Result<Self, NativeError> {
        let mut remaining = format.split(|&byte| byte == 0).next().unwrap_or_default();
        let mut items = Vec::new();
        let mut assignments = HashSet::new();
        let mut slots = 0;
        let mut positional = None;
        while let Some((&byte, tail)) = remaining.split_first() {
            remaining = tail;
            if whitespace(byte) {
                items.push(Item::Space);
                continue;
            }
            if byte != b'%' {
                items.push(Item::Literal(byte));
                continue;
            }
            if let Some(tail) = remaining.strip_prefix(b"%") {
                items.push(Item::Literal(b'%'));
                remaining = tail;
                continue;
            }
            let suppressed = remaining.starts_with(b"*");
            if suppressed {
                remaining = &remaining[1..];
            }
            let (index, after_index) = number(remaining);
            let index = match after_index.strip_prefix(b"$") {
                Some(after) if after_index.len() != remaining.len() && !suppressed => {
                    remaining = after;
                    Some(index.checked_sub(1).ok_or_else(index_out_of_range)?)
                }
                _ => None,
            };
            let (width, after) = number(remaining);
            remaining = after;
            if matches!(remaining.first(), Some(b'l' | b'L' | b'h')) {
                remaining = &remaining[1..];
            }
            let (field, after) = field(remaining)?;
            remaining = after;
            let slot = if suppressed {
                None
            } else {
                if positional.is_some_and(|used| used != index.is_some()) {
                    return Err(NativeError::value_error(
                        "cannot mix \"%\" and \"%n$\" conversion specifiers",
                    ));
                }
                positional = Some(index.is_some());
                let slot = index.unwrap_or(slots);
                if slot > 65_535 {
                    return Err(index_out_of_range());
                }
                if !assignments.insert(slot) {
                    return Err(NativeError::value_error(
                        "Variable is assigned by multiple \"%n$\" conversion specifiers",
                    ));
                }
                slots = slots.max(slot + 1);
                Some(slot)
            };
            let width = match (width, field) {
                (0, b"c") => 1,
                (0, _) => usize::MAX,
                (width, _) => width,
            };
            items.push(Item::Field { field, width, slot });
        }
        if variables != 0 && slots != variables {
            let message = if variables > slots {
                "Variable is not assigned by any conversion specifiers"
            } else if positional == Some(true) {
                return Err(index_out_of_range());
            } else {
                "Different numbers of variable names and field specifiers"
            };
            return Err(NativeError::value_error(message));
        }
        Ok(Self { items, slots })
    }
}

fn number(bytes: &[u8]) -> (usize, &[u8]) {
    let mut count = 0;
    let mut number = 0usize;
    while let Some(&byte) = bytes.get(count).filter(|b| b.is_ascii_digit()) {
        number = number
            .saturating_mul(10)
            .saturating_add(usize::from(byte - b'0'));
        count += 1;
    }
    (number, &bytes[count..])
}

fn field(bytes: &[u8]) -> Result<(&[u8], &[u8]), NativeError> {
    match bytes {
        [b'[', set @ ..] => {
            let first = usize::from(set.starts_with(b"^"));
            let first = first + usize::from(set.get(first) == Some(&b']'));
            let end = set
                .get(first..)
                .and_then(|set| set.iter().position(|&b| b == b']'))
                .map(|end| end + first + 1)
                .ok_or_else(|| NativeError::value_error("Unmatched [ in format string"))?;
            Ok((&bytes[..end], &bytes[end + 1..]))
        }
        [
            b'n' | b'd' | b'D' | b'i' | b'o' | b'x' | b'X' | b'u' | b'f' | b'e' | b'E' | b'g'
            | b's' | b'c',
            rest @ ..,
        ] => Ok((&bytes[..1], rest)),
        [byte, ..] => Err(NativeError::value_error(format!(
            "Bad scan conversion character \"{}\"",
            char::from(*byte)
        ))),
        [] => Err(NativeError::value_error("Bad scan conversion character \"")),
    }
}

fn index_out_of_range() -> NativeError {
    NativeError::value_error("\"%n$\" argument index out of range")
}
