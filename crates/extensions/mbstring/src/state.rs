//! What mbstring remembers during one request. Each request starts afresh:
//!
//! ```php
//! mb_internal_encoding('ASCII');
//! // next request:
//! mb_internal_encoding();  // "UTF-8"
//! ```

use crate::codec::Substitute;
use crate::encoding::Encoding;

#[derive(Clone)]
pub struct Settings {
    /// The encoding name a function was last given, which decides whether
    /// a transfer encoding is deprecated again.
    pub last_encoding: Option<String>,
    pub internal: Encoding,
    /// `None` is `"pass"`: `mb_output_handler` leaves output as it is.
    pub output: Option<Encoding>,
    pub regex: Encoding,
    /// The last numeric `mb_substitute_character()`, which `"long"` and
    /// `"entity"` keep for invalid bytes.
    pub substitute_character: char,
    pub substitute: Substitute,
    pub detect: Vec<Encoding>,
    pub language: &'static str,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            last_encoding: None,
            internal: Encoding::Utf8,
            output: Some(Encoding::Utf8),
            regex: Encoding::Utf8,
            substitute_character: '?',
            substitute: Substitute::default(),
            detect: vec![Encoding::Ascii, Encoding::Utf8],
            language: "neutral",
        }
    }
}
