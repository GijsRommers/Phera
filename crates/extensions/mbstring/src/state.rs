//! What mbstring remembers during one request. Each request starts afresh:
//!
//! ```php
//! mb_internal_encoding('ASCII');
//! // the next request:
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
    pub language: Language,
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
            language: Language::Neutral,
        }
    }
}

/// `mb_language()`, which decides the defaults for mail.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Neutral,
    Universal,
    Japanese,
    English,
    German,
    Russian,
    Korean,
    Armenian,
    Turkish,
    Ukrainian,
    SimplifiedChinese,
    TraditionalChinese,
}

impl Language {
    /// By name or language code, ignoring case.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name.to_ascii_lowercase().as_str() {
            "neutral" => Self::Neutral,
            "uni" | "universal" => Self::Universal,
            "japanese" | "ja" => Self::Japanese,
            "english" | "en" => Self::English,
            "german" | "de" => Self::German,
            "russian" | "ru" => Self::Russian,
            "korean" | "ko" => Self::Korean,
            "armenian" | "hy" => Self::Armenian,
            "turkish" | "tr" => Self::Turkish,
            "ukrainian" | "ua" => Self::Ukrainian,
            "simplified chinese" | "zh-cn" => Self::SimplifiedChinese,
            "traditional chinese" | "zh-tw" => Self::TraditionalChinese,
            _ => return None,
        })
    }

    /// The name `mb_language()` returns.
    pub fn name(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Universal => "uni",
            Self::Japanese => "Japanese",
            Self::English => "English",
            Self::German => "German",
            Self::Russian => "Russian",
            Self::Korean => "Korean",
            Self::Armenian => "Armenian",
            Self::Turkish => "Turkish",
            Self::Ukrainian => "Ukrainian",
            Self::SimplifiedChinese => "Simplified Chinese",
            Self::TraditionalChinese => "Traditional Chinese",
        }
    }

    /// The charset of mail headers in this language.
    pub fn mail_charset(self) -> &'static str {
        match self {
            Self::Japanese => "ISO-2022-JP",
            Self::English | Self::German => "ISO-8859-1",
            Self::Russian => "KOI8-R",
            Self::Korean => "ISO-2022-KR",
            _ => "UTF-8",
        }
    }
}
