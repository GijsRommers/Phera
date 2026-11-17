//! Decode PHP ENT bit flags once into document, quote and invalid-input policies.

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::strings) enum Document {
    Html401,
    Xml1,
    Xhtml,
    Html5,
}

impl Document {
    pub(in crate::strings) fn from_flags(flags: i64) -> Self {
        match flags & 48 {
            16 => Self::Xml1,
            32 => Self::Xhtml,
            48 => Self::Html5,
            _ => Self::Html401,
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::strings) enum Quotes {
    Neither,
    Single,
    Double,
    Both,
}

impl Quotes {
    pub(in crate::strings) fn includes(self, byte: u8) -> bool {
        match byte {
            b'\'' => matches!(self, Self::Single | Self::Both),
            b'"' => matches!(self, Self::Double | Self::Both),
            _ => true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::strings) enum InvalidInput {
    Ignore,
    Substitute,
    Reject,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::strings) enum CharacterPolicy {
    Any,
    DocumentOnly,
}

#[derive(Clone, Copy)]
pub(in crate::strings) struct Options {
    pub(in crate::strings) document: Document,
    pub(in crate::strings) quotes: Quotes,
    pub(in crate::strings) invalid: InvalidInput,
    pub(in crate::strings) characters: CharacterPolicy,
}

impl Options {
    /// PHP gives ENT_IGNORE precedence over ENT_SUBSTITUTE when both bits are set.
    pub(in crate::strings) fn from_flags(flags: i64) -> Self {
        Self {
            document: Document::from_flags(flags),
            quotes: match flags & 3 {
                1 => Quotes::Single,
                2 => Quotes::Double,
                3 => Quotes::Both,
                _ => Quotes::Neither,
            },
            invalid: if flags & 4 != 0 {
                InvalidInput::Ignore
            } else if flags & 8 != 0 {
                InvalidInput::Substitute
            } else {
                InvalidInput::Reject
            },
            characters: if flags & 128 != 0 {
                CharacterPolicy::DocumentOnly
            } else {
                CharacterPolicy::Any
            },
        }
    }
}
