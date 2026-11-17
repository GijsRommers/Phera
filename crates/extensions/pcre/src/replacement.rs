//! Substitutions accept `$1`, `${1}` and `\1`; `preg_replace('/(a)/', '$1!', 'a')`.

use crate::scan::Captures;

pub(crate) fn expand(template: &[u8], subject: &[u8], captures: &Captures) -> Vec<u8> {
    let mut output = Vec::new();
    let mut position = 0;
    while position < template.len() {
        let byte = template[position];
        if byte == b'\\' && matches!(template.get(position + 1), Some(b'\\' | b'$')) {
            output.push(template[position + 1]);
            position += 2;
            continue;
        }
        if matches!(byte, b'\\' | b'$') {
            let mut cursor = position + 1;
            let braces = byte == b'$' && template.get(cursor) == Some(&b'{');
            if braces {
                cursor += 1;
            }
            if let Some(digit) = template.get(cursor).filter(|byte| byte.is_ascii_digit()) {
                let mut number = usize::from(*digit - b'0');
                cursor += 1;
                if let Some(digit) = template.get(cursor).filter(|byte| byte.is_ascii_digit()) {
                    number = number * 10 + usize::from(*digit - b'0');
                    cursor += 1;
                }
                if !braces || template.get(cursor) == Some(&b'}') {
                    if let Some(Some((start, end))) = captures.get(number) {
                        output.extend_from_slice(&subject[*start..*end]);
                    }
                    position = cursor + usize::from(braces);
                    continue;
                }
            }
        }
        output.push(byte);
        position += 1;
    }
    output
}
