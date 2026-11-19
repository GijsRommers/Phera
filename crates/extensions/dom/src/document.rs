//! Document settings and the arena shared by XML and HTML readers.
//!
//! ```php
//! $document = new DOMDocument('1.0', 'UTF-8');
//! $document->formatOutput = true;
//! ```

use crate::tree::Arena;

#[derive(Clone, Debug)]
pub struct Settings {
    pub version: Option<String>,
    pub encoding: Option<String>,
    pub standalone: Option<bool>,
    pub uri: Option<String>,
    pub html: bool,
    pub format: bool,
    pub preserve_whitespace: bool,
    pub strict: bool,
    pub recover: bool,
    pub resolve_externals: bool,
    pub substitute_entities: bool,
    pub validate_on_parse: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: Some("1.0".into()),
            encoding: None,
            standalone: None,
            uri: None,
            html: false,
            format: false,
            preserve_whitespace: true,
            strict: true,
            recover: false,
            resolve_externals: false,
            substitute_entities: false,
            validate_on_parse: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct Document {
    pub arena: Arena,
    pub settings: Settings,
}

#[derive(Clone, Debug)]
pub struct ParseError {
    pub level: i64,
    pub code: i64,
    pub column: usize,
    pub message: String,
    pub file: String,
    pub line: usize,
}

impl ParseError {
    pub(crate) fn fatal(code: i64, message: impl Into<String>) -> Self {
        Self {
            level: 3,
            code,
            column: 1,
            message: message.into(),
            file: String::new(),
            line: 1,
        }
    }
}
