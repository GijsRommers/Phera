//! Share byte decoding and encoding with intl's UConverter.
//!
//! ```php
//! UConverter::transcode('é', 'UTF-16BE', 'UTF-8'); // "\x00\xE9"
//! ```

use crate::{codec, encoding::Encoding};

/// A Unicode codec with separate handling of byte-order marks at the caller.
#[derive(Clone, Copy)]
pub struct UnicodeCodec(Encoding);

impl UnicodeCodec {
    pub fn find(name: &str) -> Option<Self> {
        let encoding = Encoding::find(name)?;
        matches!(
            encoding,
            Encoding::Utf8
                | Encoding::Utf16
                | Encoding::Utf16Be
                | Encoding::Utf16Le
                | Encoding::Utf32
                | Encoding::Utf32Be
                | Encoding::Utf32Le
        )
        .then_some(Self(encoding))
    }

    pub fn decode(self, bytes: &[u8]) -> Vec<u32> {
        codec::decode(bytes, self.0)
    }

    pub fn encode(self, points: &[u32]) -> Vec<u8> {
        codec::encode(points, self.0, &codec::Substitute::Character('\u{fffd}'))
    }
}
