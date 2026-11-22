//! String functions. PHP strings are bytes, not characters: every function
//! here works on bytes, and none of them looks at the locale.
//!
//! The helpers below are what several modules share: the size limit every
//! growing string checks, and substring search.

use phera_extension_api::{NativeError, Registry};

mod alphanumeric;
pub(crate) mod base64;
mod case;
mod compare;
mod decimal;
mod encode;
mod escaping;
pub(crate) mod format;
mod hebrew;
mod html;
mod join;
mod metaphone;
mod number_format;
mod padding;
mod quoted_printable;
mod replace;
mod search;
mod similarity;
mod split;
mod substitute;
mod substring;
mod tags;
mod text;
pub(crate) mod tokens;
mod trim;
pub(crate) mod url;
mod uuencode;

/// The longest string a function here builds before failing as PHP fails
/// for a string past its memory limit.
pub(crate) const MAX_STRING_BYTES: usize = 2 * 1024 * 1024 * 1024;

pub(crate) fn register(registry: &mut Registry) {
    registry.function(hebrew::ENTRY);
    registry.function(quoted_printable::ENCODE);
    registry.function(quoted_printable::DECODE);
    registry.function(encode::UTF8_DECODE);
    for entry in [
        alphanumeric::FUNCTIONS,
        base64::FUNCTIONS,
        case::FUNCTIONS,
        compare::FUNCTIONS,
        encode::FUNCTIONS,
        escaping::FUNCTIONS,
    ]
    .into_iter()
    .flatten()
    {
        registry.function(*entry);
    }
    format::register(registry);
    join::register_sync_selectors(registry);
    let modules = [
        html::FUNCTIONS,
        metaphone::FUNCTIONS,
        number_format::FUNCTIONS,
        padding::FUNCTIONS,
        replace::FUNCTIONS,
        search::FUNCTIONS,
        similarity::FUNCTIONS,
        split::FUNCTIONS,
        substitute::FUNCTIONS,
        substring::FUNCTIONS,
        tags::FUNCTIONS,
        text::FUNCTIONS,
        tokens::FUNCTIONS,
        trim::FUNCTIONS,
        url::FUNCTIONS,
        uuencode::FUNCTIONS,
    ];
    for entry in modules.into_iter().flatten() {
        registry.function(*entry);
    }
    registry.describe_function("utf8_encode", "string", None);
    registry.describe_function("utf8_decode", "string", None);
    replace::register_sync_selectors(registry);
}

/// PHP's error for a string that would pass [`MAX_STRING_BYTES`].
pub(super) fn check_size(length: usize) -> Result<(), NativeError> {
    if length > MAX_STRING_BYTES {
        return Err(NativeError::error(
            "String size exceeds the maximum allowed size",
        ));
    }
    Ok(())
}

/// Makes room for `additional` more bytes, or fails as PHP does when the
/// string would grow too large or memory runs out.
pub(super) fn reserve(result: &mut Vec<u8>, additional: usize) -> Result<(), NativeError> {
    check_size(result.len().saturating_add(additional))?;
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

/// The first position of `needle` in `haystack`; an empty needle is found
/// at 0, as in PHP.
pub(super) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    CaseSensitivity::Sensitive.find(haystack, needle)
}

/// Whether a search tells upper and lower case apart. PHP's case-insensitive
/// functions (`stripos`, `str_ireplace`, `strcasecmp`) fold the ASCII
/// letters only, whatever the locale.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CaseSensitivity {
    Sensitive,
    Insensitive,
}

impl CaseSensitivity {
    /// The first position of `needle` in `haystack`; an empty needle is
    /// found at 0.
    pub(super) fn find(self, haystack: &[u8], needle: &[u8]) -> Option<usize> {
        match (self, needle) {
            (_, []) => Some(0),
            (Self::Sensitive, [byte]) => memchr::memchr(*byte, haystack),
            (Self::Sensitive, _) if haystack.starts_with(needle) => Some(0),
            (Self::Sensitive, _) => memchr::memmem::find(haystack, needle),
            (Self::Insensitive, _) => haystack
                .windows(needle.len())
                .position(|candidate| candidate.eq_ignore_ascii_case(needle)),
        }
    }

    /// The last position of the needle, including the end for an empty needle.
    pub(super) fn rfind(self, haystack: &[u8], needle: &[u8]) -> Option<usize> {
        match (self, needle) {
            (_, []) => Some(haystack.len()),
            (Self::Sensitive, [byte]) => memchr::memrchr(*byte, haystack),
            (Self::Sensitive, _) if haystack.ends_with(needle) => {
                Some(haystack.len() - needle.len())
            }
            (Self::Sensitive, _) => memchr::memmem::rfind(haystack, needle),
            (Self::Insensitive, _) => haystack
                .windows(needle.len())
                .rposition(|candidate| candidate.eq_ignore_ascii_case(needle)),
        }
    }
}

/// Match PHP's startup LC_CTYPE alphabet for word and phonetic boundaries.
/// Darwin classifies these Latin-1 bytes as letters; glibc's UTF-8 locale does not.
pub(super) fn alphabetic(byte: u8) -> bool {
    byte.is_ascii_alphabetic()
        || cfg!(target_vendor = "apple")
            && matches!(byte, 170 | 181 | 186 | 192..=214 | 216..=246 | 248..=255)
}
