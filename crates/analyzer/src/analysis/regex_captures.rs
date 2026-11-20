//! In rows from `preg_match_all('/(a)(b)?/', $s, $m, PREG_SET_ORDER |
//! PREG_UNMATCHED_AS_NULL)`, capture 1 participates and capture 2 may be null.

use crate::types::{Field, ShapeKey, ValueType};

/// Infer required captures for literal patterns with numbered and non-capturing
/// groups. Leave alternatives, PCRE directives and other group syntax unresolved.
pub(super) fn required_captures(pattern: &[u8]) -> Option<Vec<Field>> {
    let delimiter = *pattern.first()?;
    if !delimiter.is_ascii_punctuation() || b"\\([{<>}])".contains(&delimiter) {
        return None;
    }
    let end = pattern.iter().rposition(|byte| *byte == delimiter)?;
    if end == 0
        || !pattern[end + 1..]
            .iter()
            .all(|byte| b"imsuADUS".contains(byte))
    {
        return None;
    }

    let mut captures = vec![true];
    let mut groups = Vec::new();
    let mut body = pattern[1..end].iter().copied().peekable();
    while let Some(byte) = body.next() {
        match byte {
            b'\\' => skip_escape(body.next()?)?,
            b'[' => {
                if body.peek() == Some(&b'^') {
                    body.next();
                }
                if body.peek() == Some(&b']') {
                    body.next();
                }
                loop {
                    match body.next()? {
                        b']' => break,
                        b'\\' => skip_escape(body.next()?)?,
                        b'[' => return None,
                        _ => {}
                    }
                }
            }
            b'(' => {
                groups.push(captures.len());
                match body.peek() {
                    Some(b'*') => return None,
                    Some(b'?') => {
                        body.next();
                        if body.next() != Some(b':') {
                            return None;
                        }
                    }
                    _ if captures.len() < 32 => captures.push(true),
                    _ => return None,
                }
            }
            b')' => {
                let first = groups.pop()?;
                if matches!(body.peek(), Some(b'?' | b'*' | b'{')) {
                    captures[first..].fill(false);
                }
            }
            b'|' => return None,
            _ => {}
        }
    }
    if !groups.is_empty() {
        return None;
    }
    Some(
        captures
            .into_iter()
            .enumerate()
            .filter_map(|(slot, required)| {
                required.then_some(Field {
                    key: ShapeKey::Integer(slot as i64),
                    value: ValueType::String,
                    optional: false,
                })
            })
            .collect(),
    )
}

fn skip_escape(byte: u8) -> Option<()> {
    (!matches!(byte, b'Q' | b'E' | b'c' | b'g' | b'k')).then_some(())
}
