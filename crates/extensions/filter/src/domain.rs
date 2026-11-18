//! Domain validation enforces length even without hostname character rules.
//!
//! ```php
//! filter_var('this_is_a_domain', FILTER_VALIDATE_DOMAIN); // 'this_is_a_domain'
//! filter_var('this_is_a_domain', FILTER_VALIDATE_DOMAIN, FILTER_FLAG_HOSTNAME); // false
//! ```

use phera_extension_api::NativeError;

use crate::options::Options;

pub fn valid(input: &[u8], hostname: bool) -> bool {
    let domain = input.strip_suffix(b".").unwrap_or(input);
    if domain.len() > 253 || input.starts_with(b".") {
        return false;
    }
    if domain.is_empty() {
        return !hostname;
    }
    let mut labels = domain.split(|&byte| byte == b'.').peekable();
    while let Some(label) = labels.next() {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if hostname {
            let final_dot = labels.peek().is_none() && input.ends_with(b".");
            if !label.first().is_some_and(u8::is_ascii_alphanumeric)
                || (!final_dot && !label.last().is_some_and(u8::is_ascii_alphanumeric))
                || !label
                    .iter()
                    .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
            {
                return false;
            }
        }
    }
    true
}

pub fn mac(input: &[u8], options: &Options) -> Result<bool, NativeError> {
    let separator = options.string("separator");
    let expected = match separator.as_ref().map(|value| value.as_bytes()) {
        None => None,
        Some(&[byte]) => Some(byte),
        Some(_) => return Err(options.invalid("\"separator\" option must be one character long")),
    };
    let (separator, width) = match input {
        [_, _, _, _, b'.', ..] if input.len() == 14 => (b'.', 4),
        [_, _, separator @ (b':' | b'-'), ..] if input.len() == 17 => (*separator, 2),
        _ => return Ok(false),
    };
    Ok(expected.is_none_or(|byte| byte == separator)
        && input
            .split(|&byte| byte == separator)
            .all(|part| part.len() == width && part.iter().all(u8::is_ascii_hexdigit)))
}
