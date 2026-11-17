//! The replacement string of `preg_replace`: `$1`, `${1}` and `\1` stand for
//! a group, `\\` and `\$` for a backslash and a dollar sign.
//!
//! ```php
//! preg_replace('/(a)(b)?/', '[$1|${2}|\1]', 'a');   // "[a||a]"
//! preg_replace('/(a)/', '${1}0', 'a');              // "a0", where $10 is group 10
//! ```
//!
//! A group number has at most two digits; a group that did not take part,
//! or does not exist, stands for nothing.

use crate::scan::Captures;

/// Appends `template` to `output`, with the groups of `captures` filled in.
pub(crate) fn expand(output: &mut Vec<u8>, template: &[u8], subject: &[u8], captures: &Captures) {
    let mut position = 0;
    while position < template.len() {
        let byte = template[position];
        if byte == b'\\' && matches!(template.get(position + 1), Some(b'\\' | b'$')) {
            output.push(template[position + 1]);
            position += 2;
            continue;
        }
        if matches!(byte, b'\\' | b'$')
            && let Some((group, length)) = group_reference(&template[position..])
        {
            if let Some(Some((start, end))) = captures.get(group) {
                output.extend_from_slice(&subject[*start..*end]);
            }
            position += length;
            continue;
        }
        output.push(byte);
        position += 1;
    }
}

/// The group number and length of a reference at the start of `text`:
/// `$1`, `$12`, `${12}`, `\1` or `\12`.
fn group_reference(text: &[u8]) -> Option<(usize, usize)> {
    let braces = text.starts_with(b"${");
    let digits_start = if braces { 2 } else { 1 };
    let digits = text[digits_start..]
        .iter()
        .take(2)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 {
        return None;
    }
    let group = text[digits_start..digits_start + digits]
        .iter()
        .fold(0, |number, digit| number * 10 + usize::from(digit - b'0'));
    let mut length = digits_start + digits;
    if braces {
        if text.get(length) != Some(&b'}') {
            return None;
        }
        length += 1;
    }
    Some((group, length))
}
