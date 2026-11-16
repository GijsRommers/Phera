//! PHP's five-state HTML/PHP tag stripper, with literal allowed-tag preservation.
use super::{buffer, bytes, find, reserve};
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
    value::to_php_string,
};
const PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("allowed_tags", ParamType::ArrayOrString)
        .nullable()
        .optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("strip_tags", PARAMS, strip));
}
/// Tag matching normalizes the name but leaves allowed attributes in the output.
fn permitted(tag: &[u8], allowed: &[u8]) -> bool {
    let mut normalized = Vec::new();
    let mut started = false;
    for (index, &byte) in tag.iter().enumerate() {
        let byte = byte.to_ascii_lowercase();
        match byte {
            b'<' => normalized.push(byte),
            b'>' => break,
            _ => {
                if byte.is_ascii_whitespace() {
                    if started {
                        break;
                    }
                } else {
                    started = true;
                    if byte != b'/'
                        || index.checked_sub(1).and_then(|i| tag.get(i)) != Some(&b'<')
                            && tag.get(index + 1) != Some(&b'>')
                    {
                        normalized.push(byte);
                    }
                }
            }
        }
    }
    normalized.push(b'>');
    find(allowed, &normalized).is_some()
}
/// Nested brackets, quotes, processing instructions, declarations and comments use
/// separate states, matching php_strip_tags_ex rather than an HTML parser.
fn strip(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let input = args::bytes(arguments, 0);
    let mut allowed = Vec::new();
    if let Some(array) = args::array(arguments, 1) {
        for value in array.values() {
            let string = to_php_string(value, context.diagnostics())?;
            reserve(&mut allowed, string.len().saturating_add(2))?;
            allowed.push(b'<');
            allowed.extend_from_slice(string.as_bytes());
            allowed.push(b'>');
        }
    } else {
        allowed.extend_from_slice(args::bytes(arguments, 1));
    }
    allowed.make_ascii_lowercase();
    if let Some(end) = allowed.iter().position(|&byte| byte == 0) {
        allowed.truncate(end);
    }
    let mut output = buffer(input.len())?;
    let mut tag = buffer(input.len())?;
    let mut state = 0;
    let mut quote = 0u8;
    let mut significant = 0u8;
    let mut depth = 0usize;
    let mut brackets = 0i64;
    let mut xml = false;
    for (cursor, &byte) in input.iter().enumerate() {
        let previous = cursor
            .checked_sub(1)
            .and_then(|index| input.get(index))
            .copied()
            .unwrap_or_default();
        match state {
            0 => match byte {
                0 => {}
                b'<' if quote == 0 => {
                    if input.get(cursor + 1).is_some_and(u8::is_ascii_whitespace) {
                        output.push(byte);
                    } else {
                        significant = b'<';
                        state = 1;
                        if !allowed.is_empty() {
                            tag.push(byte);
                        }
                    }
                }
                b'<' => {}
                b'>' if depth > 0 => {
                    depth -= 1;
                }
                b'>' if quote != 0 => {}
                _ => output.push(byte),
            },
            1 => {
                let mut regular = false;
                match byte {
                    0 => {}
                    b'<' => {
                        if quote == 0 {
                            if input.get(cursor + 1).is_some_and(u8::is_ascii_whitespace) {
                                regular = true;
                            } else {
                                depth += 1;
                            }
                        }
                    }
                    b'>' => {
                        if depth > 0 {
                            depth -= 1;
                        } else if quote == 0 {
                            significant = b'>';
                            if !(xml && previous == b'-') {
                                quote = 0;
                                state = 0;
                                xml = false;
                                if !allowed.is_empty() {
                                    tag.push(b'>');
                                    if permitted(&tag, &allowed) {
                                        output.extend_from_slice(&tag);
                                    }
                                    tag.clear();
                                }
                            }
                        }
                    }
                    b'\'' | b'"' => {
                        if quote == 0 || quote == byte {
                            quote = if quote == 0 { byte } else { 0 };
                        }
                        regular = true;
                    }
                    b'!' if previous == b'<' => {
                        state = 3;
                        significant = byte;
                    }
                    b'?' if previous == b'<' => {
                        brackets = 0;
                        state = 2;
                    }
                    _ => regular = true,
                }
                if regular && !allowed.is_empty() {
                    tag.push(byte);
                }
            }
            2 => match byte {
                b'(' if !matches!(significant, b'"' | b'\'') => {
                    significant = byte;
                    brackets = brackets.saturating_add(1);
                }
                b')' if !matches!(significant, b'"' | b'\'') => {
                    significant = byte;
                    brackets = brackets.saturating_sub(1);
                }
                b'>' if depth > 0 => {
                    depth -= 1;
                }
                b'>' if quote == 0 && brackets == 0 && significant != b'"' && previous == b'?' => {
                    quote = 0;
                    state = 0;
                    tag.clear();
                }
                b'\'' | b'"' if previous != b'\\' => {
                    if significant == byte {
                        significant = 0;
                    } else if significant != b'\\' {
                        significant = byte;
                    }
                    if quote == 0 || quote == byte {
                        quote = if quote == 0 { byte } else { 0 };
                    }
                }
                b'l' | b'L'
                    if cursor > 4
                        && input
                            .get(cursor - 4..=cursor)
                            .is_some_and(|part| part.eq_ignore_ascii_case(b"<?xml")) =>
                {
                    state = 1;
                    xml = true;
                }
                _ => {}
            },
            3 => match byte {
                b'>' if depth > 0 => {
                    depth -= 1;
                }
                b'>' if quote == 0 => {
                    quote = 0;
                    state = 0;
                    tag.clear();
                }
                b'\'' | b'"' if previous != b'\\' && (quote == 0 || quote == byte) => {
                    quote = if quote == 0 { byte } else { 0 };
                }
                b'-' if cursor >= 2 && input.get(cursor - 2..cursor) == Some(b"!-".as_slice()) => {
                    state = 4
                }
                b'e' | b'E'
                    if cursor > 6
                        && input
                            .get(cursor - 6..=cursor)
                            .is_some_and(|part| part.eq_ignore_ascii_case(b"doctype")) =>
                {
                    state = 1
                }
                _ => {}
            },
            _ => {
                if byte == b'>'
                    && quote == 0
                    && cursor >= 2
                    && input.get(cursor - 2..cursor) == Some(b"--".as_slice())
                {
                    quote = 0;
                    state = 0;
                    tag.clear();
                }
            }
        }
    }
    Ok(bytes(output))
}
