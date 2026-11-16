//! Byte string operations, with shared allocation and substring search helpers.

use phera_extension_api::{NativeError, PhpString, Registry, Value};

mod case;
mod decimal;
mod encode;
mod format;
mod number_format;
mod replace;
mod search;
mod split;
mod trim;
mod url;

pub(crate) const MAX_STRING_BYTES: usize = 2 * 1024 * 1024 * 1024;

pub(crate) fn register(registry: &mut Registry) {
    number_format::register(registry);
    format::register(registry);
    url::register(registry);
    encode::register(registry);
    case::register(registry);
    replace::register(registry);
    trim::register(registry);
    split::register(registry);
    search::register(registry);
}

pub(super) fn bytes(value: impl Into<Vec<u8>>) -> Value {
    Value::String(PhpString::from(value.into()))
}

/// Reserve before writing so impossible string sizes become PHP exceptions.
pub(super) fn reserve(result: &mut Vec<u8>, additional: usize) -> Result<(), NativeError> {
    if result.len().saturating_add(additional) > MAX_STRING_BYTES {
        return Err(NativeError::error(
            "String size exceeds the maximum allowed size",
        ));
    }
    result
        .try_reserve(additional)
        .map_err(|_| NativeError::error("Could not allocate string"))
}

/// All growing string operations share the same memory bound and error path.
pub(super) fn buffer(length: usize) -> Result<Vec<u8>, NativeError> {
    let mut result = Vec::new();
    reserve(&mut result, length)?;
    Ok(result)
}

/// Skip directly to candidate first bytes; an empty needle matches at zero.
pub(super) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes(haystack, needle, false)
}

/// PHP case-insensitive byte searches fold ASCII only, independent of locale.
pub(super) fn find_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes(haystack, needle, true)
}

fn find_bytes(haystack: &[u8], needle: &[u8], insensitive: bool) -> Option<usize> {
    let Some(first) = needle.first() else {
        return Some(0);
    };
    let last = haystack.len().checked_sub(needle.len())?;
    haystack
        .iter()
        .take(last + 1)
        .enumerate()
        .find_map(|(index, byte)| {
            if if insensitive {
                byte.eq_ignore_ascii_case(first)
            } else {
                byte == first
            } {
                let candidate = haystack.get(index..index + needle.len())?;
                if if insensitive {
                    candidate.eq_ignore_ascii_case(needle)
                } else {
                    candidate == needle
                } {
                    return Some(index);
                }
            }
            None
        })
}
