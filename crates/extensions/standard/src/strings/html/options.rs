//! The `$flags` of `htmlspecialchars` and `htmlspecialchars_decode`, parsed
//! into what each bit group decides.

use crate::constants::{
    ENT_COMPAT, ENT_DISALLOWED, ENT_HTML5, ENT_IGNORE, ENT_QUOTES, ENT_SUBSTITUTE, ENT_XHTML,
    ENT_XML1,
};

/// The document type, which decides which entities and characters are valid.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Document {
    Html401,
    Xml1,
    Xhtml,
    Html5,
}

/// Which quotes are encoded and decoded; `&`, `<` and `>` always are.
#[derive(Clone, Copy)]
pub(super) enum Quotes {
    Neither,
    Single,
    Double,
    Both,
}

impl Quotes {
    pub(super) fn includes(self, byte: u8) -> bool {
        match byte {
            b'\'' => matches!(self, Self::Single | Self::Both),
            b'"' => matches!(self, Self::Double | Self::Both),
            _ => true,
        }
    }
}

/// What happens to bytes that are not valid in the encoding.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InvalidInput {
    /// `ENT_IGNORE`: dropped.
    Ignore,
    /// `ENT_SUBSTITUTE`: replaced by U+FFFD.
    Substitute,
    /// Neither: the whole result is an empty string.
    Reject,
}

#[derive(Clone, Copy)]
pub(super) struct Options {
    pub(super) document: Document,
    pub(super) quotes: Quotes,
    pub(super) invalid: InvalidInput,
    /// `ENT_DISALLOWED`: characters the document type does not allow are
    /// replaced by U+FFFD.
    pub(super) replace_disallowed: bool,
}

impl Options {
    pub(super) fn from_flags(flags: i64) -> Self {
        // ENT_HTML5 is ENT_XML1 | ENT_XHTML, and ENT_HTML401 is neither bit.
        let document = match flags & ENT_HTML5 {
            ENT_XML1 => Document::Xml1,
            ENT_XHTML => Document::Xhtml,
            ENT_HTML5 => Document::Html5,
            _ => Document::Html401,
        };
        // ENT_QUOTES is the two quote bits; the single-quote bit has no name
        // of its own.
        let quotes = match flags & ENT_QUOTES {
            ENT_QUOTES => Quotes::Both,
            ENT_COMPAT => Quotes::Double,
            0 => Quotes::Neither,
            _ => Quotes::Single,
        };
        // With both bits set, ENT_IGNORE wins.
        let invalid = if flags & ENT_IGNORE != 0 {
            InvalidInput::Ignore
        } else if flags & ENT_SUBSTITUTE != 0 {
            InvalidInput::Substitute
        } else {
            InvalidInput::Reject
        };
        Self {
            document,
            quotes,
            invalid,
            replace_disallowed: flags & ENT_DISALLOWED != 0,
        }
    }
}

impl Document {
    /// Whether the document type allows this character. Tab, line feed
    /// and carriage return always pass; the types differ on the C0 and C1
    /// controls and on Unicode's noncharacters.
    pub(super) fn allows(self, code: u32) -> bool {
        if matches!(code, 0x09 | 0x0a | 0x0d) {
            return true;
        }
        match self {
            Document::Xml1 | Document::Xhtml => {
                (0x20..=0xd7ff).contains(&code)
                    || (0xe000..=0x10ffff).contains(&code) && !matches!(code, 0xfffe | 0xffff)
            }
            Document::Html5 => {
                code == 0x0c
                    || (0x20..=0x7e).contains(&code)
                    || (0xa0..=0xd7ff).contains(&code)
                    || (0xe000..=0x10ffff).contains(&code)
                        && code & 0xffff < 0xfffe
                        && !(0xfdd0..=0xfdef).contains(&code)
            }
            Document::Html401 => {
                (0x20..=0x7e).contains(&code)
                    || (0xa0..=0xd7ff).contains(&code)
                    || (0xe000..=0x10ffff).contains(&code)
            }
        }
    }
}
