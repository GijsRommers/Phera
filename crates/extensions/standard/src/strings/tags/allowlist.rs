//! strip_tags' allow-list: building it from the argument, and checking a
//! tag against it.

use phera_extension_api::{Context, NativeError, Value, args, value::to_php_string};

use super::super::{find, reserve};

/// Whether `tag`, as written in the input, is on the allow-list.
///
/// The tag is reduced to its lower-case name, `<b class=x>` to `<b>` and
/// `</b>` to `<b>`, and looked up in `allowed`, which is `<a><b>…`.
pub(super) fn permitted(tag: &[u8], allowed: &[u8]) -> bool {
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

/// The allow-list as one lower-case string `<a><b>…`: an array's entries are
/// strings to wrap in `<>`, a string is used as it is. Like the C string it
/// is in PHP, it ends at its first NUL.
pub(super) fn allowed_tags(
    context: &mut dyn Context,
    arguments: &[Value],
) -> Result<Vec<u8>, NativeError> {
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
    Ok(allowed)
}
