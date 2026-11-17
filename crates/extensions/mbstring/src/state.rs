//! Each request starts with `mb_internal_encoding() === 'UTF-8'`.
use crate::{encode::Substitute, encoding::Encoding};
#[derive(Clone)]
pub struct Settings {
    pub last_encoding: Option<String>,
    pub internal: Encoding,
    pub output: Encoding,
    pub regex: Encoding,
    pub substitute_codepoint: u32,
    pub substitute: Substitute,
    pub detect: Vec<Encoding>,
    pub language: &'static str,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            last_encoding: None,
            internal: Encoding::UTF8,
            output: Encoding::UTF8,
            regex: Encoding::UTF8,
            substitute_codepoint: 63,
            substitute: Substitute::default(),
            detect: vec![Encoding(15), Encoding::UTF8],
            language: "neutral",
        }
    }
}
