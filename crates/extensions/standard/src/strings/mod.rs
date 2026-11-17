//! Byte string operations, with shared allocation and substring search helpers.

use phera_extension_api::{NativeError, NativeResult, PhpArray, PhpString, Registry, Value};

mod base64;
mod case;
mod decimal;
mod encode;
mod escaping;
mod format;
mod html;
mod html_encoding;
mod html_entities;
mod metaphone;
mod number_format;
mod padding;
mod replace;
mod search;
mod search_more;
mod similarity;
mod split;
mod substitute;
mod tags;
mod text;
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
    search_more::register(registry);
    padding::register(registry);
    escaping::register(registry);
    html::register(registry);
    text::register(registry);
    base64::register(registry);
    tags::register(registry);
    similarity::register(registry);
    metaphone::register(registry);
    substitute::register(registry);
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
    find_bytes(haystack, needle, CaseSensitivity::Sensitive)
}

/// PHP case-insensitive byte searches fold ASCII only, independent of locale.
pub(super) fn find_ignore_case(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes(haystack, needle, CaseSensitivity::Insensitive)
}

/// PHP byte searches fold ASCII without consulting the process locale.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CaseSensitivity {
    Sensitive,
    Insensitive,
}

impl CaseSensitivity {
    pub(super) fn matches(self, left: &[u8], right: &[u8]) -> bool {
        match self {
            Self::Sensitive => left == right,
            Self::Insensitive => left.eq_ignore_ascii_case(right),
        }
    }
}

/// Empty needles match at zero; windows preserve byte offsets even in invalid UTF-8.
fn find_bytes(haystack: &[u8], needle: &[u8], sensitivity: CaseSensitivity) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|candidate| sensitivity.matches(candidate, needle))
}

/// Match the oracle's fixed byte alphabet for word and phonetic boundaries without
/// consulting process locale; case-insensitive comparisons remain ASCII-only.
pub(super) fn alphabetic(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
        || matches!(byte, 170 | 181 | 186 | 192..=214 | 216..=246 | 248..=255)
}

/// List-producing string functions assign consecutive integer keys in insertion order.
pub(super) fn list_array(values: impl IntoIterator<Item = Value>) -> NativeResult {
    let mut array = PhpArray::new();
    for value in values {
        array.push(value)?;
    }
    Ok(Value::from(array))
}
