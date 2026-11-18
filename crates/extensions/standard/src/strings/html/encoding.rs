//! The character encodings `htmlspecialchars` knows, and how it steps over
//! a string in each: one character at a time, and over invalid bytes in
//! exactly the steps PHP takes (ext/standard/html.c, `get_next_char`).

use phera_extension_api::Context;

use super::options::Document;

/// The next character of a string, or the invalid bytes PHP skips at once.
pub(super) enum Decoded {
    Valid { code: u32, length: usize },
    Invalid { length: usize },
}

pub(super) enum Encoding {
    Utf8,
    Latin1,
    SingleByte,
    Big5,
    Big5Hkscs,
    Gb2312,
    Sjis,
    Eucjp,
}

impl Encoding {
    /// The encoding `$encoding` names, ignoring case and anything after a
    /// NUL. An unknown name warns and means UTF-8, as an empty one does.
    pub(super) fn resolve(context: &mut dyn Context, name: &[u8]) -> Self {
        let end = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        let name = &name[..end];
        match name.to_ascii_lowercase().as_slice() {
            b"" | b"utf-8" => Self::Utf8,
            b"iso-8859-1" | b"iso8859-1" => Self::Latin1,
            b"iso-8859-15" | b"iso8859-15" => Self::SingleByte,
            b"cp1252" | b"windows-1252" | b"1252" => Self::SingleByte,
            b"cp1251" | b"windows-1251" | b"win-1251" => Self::SingleByte,
            b"koi8-r" | b"koi8-ru" | b"koi8r" => Self::SingleByte,
            b"iso8859-5" | b"iso-8859-5" => Self::SingleByte,
            b"cp866" | b"866" | b"ibm866" => Self::SingleByte,
            b"macroman" => Self::SingleByte,
            b"big5" | b"950" => Self::Big5,
            b"big5-hkscs" => Self::Big5Hkscs,
            b"gb2312" | b"936" => Self::Gb2312,
            b"shift_jis" | b"sjis" | b"932" | b"sjis-win" | b"cp932" => Self::Sjis,
            b"eucjp" | b"euc-jp" | b"eucjp-win" => Self::Eucjp,
            _ => {
                context.warning(format!(
                    "htmlspecialchars(): Charset \"{}\" is not supported, assuming UTF-8",
                    String::from_utf8_lossy(name)
                ));
                Self::Utf8
            }
        }
    }

    /// U+FFFD, the replacement character: in UTF-8 itself, in any other
    /// encoding as an entity.
    pub(super) fn replacement(&self) -> &'static [u8] {
        if matches!(self, Self::Utf8) {
            b"\xef\xbf\xbd"
        } else {
            b"&#xFFFD;"
        }
    }

    /// Whether `ENT_DISALLOWED` replaces this character. PHP checks the
    /// multibyte Asian encodings and the other single-byte ones only below
    /// 0x7e, since it cannot map their characters to Unicode.
    pub(super) fn disallowed(&self, code: u32, document: Document) -> bool {
        if matches!(
            self,
            Self::SingleByte
                | Self::Big5
                | Self::Big5Hkscs
                | Self::Gb2312
                | Self::Sjis
                | Self::Eucjp
        ) && code > 0x7d
        {
            false
        } else {
            !document.allows(code)
        }
    }

    /// The character at the start of `input`, which is not empty. A bad
    /// continuation byte is not skipped with the invalid lead byte, so the
    /// next step reads it again.
    pub(super) fn next(&self, input: &[u8]) -> Decoded {
        let first = input[0];
        let second = input.get(1).copied();
        let trail = |byte: u8| (0xa1..=0xfe).contains(&byte);
        match self {
            Self::Utf8 => utf8(input),
            Self::Latin1 | Self::SingleByte => Decoded::Valid {
                code: u32::from(first),
                length: 1,
            },
            Self::Big5 | Self::Big5Hkscs if (0x81..=0xfe).contains(&first) => match second {
                Some(next) if (0x40..=0x7e).contains(&next) || trail(next) => Decoded::Valid {
                    code: (u32::from(first) << 8) | u32::from(next),
                    length: 2,
                },
                Some(next) if matches!(self, Self::Big5Hkscs) && matches!(next, 0x80 | 0xff) => {
                    Decoded::Invalid { length: 2 }
                }
                _ => Decoded::Invalid { length: 1 },
            },
            Self::Gb2312 if trail(first) => match second {
                Some(next) if trail(next) => Decoded::Valid {
                    code: (u32::from(first) << 8) | u32::from(next),
                    length: 2,
                },
                Some(0x8e | 0x8f | 0xa0 | 0xff) => Decoded::Invalid { length: 2 },
                _ => Decoded::Invalid { length: 1 },
            },
            Self::Gb2312 if matches!(first, 0x8e | 0x8f | 0xa0 | 0xff) => {
                Decoded::Invalid { length: 1 }
            }
            Self::Sjis if (0x81..=0x9f).contains(&first) || (0xe0..=0xfc).contains(&first) => {
                match second {
                    Some(next) if (0x40..0xfd).contains(&next) && next != 0x7f => Decoded::Valid {
                        code: (u32::from(first) << 8) | u32::from(next),
                        length: 2,
                    },
                    Some(0x80 | 0xa0 | 0xfd..=0xff) => Decoded::Invalid { length: 2 },
                    _ => Decoded::Invalid { length: 1 },
                }
            }
            Self::Sjis if first >= 0x80 && !(0xa1..=0xdf).contains(&first) => {
                Decoded::Invalid { length: 1 }
            }
            Self::Eucjp if trail(first) || first == 0x8e => match second {
                Some(next)
                    if (if first == 0x8e {
                        (0xa1..=0xdf).contains(&next)
                    } else {
                        trail(next)
                    }) =>
                {
                    Decoded::Valid {
                        code: (u32::from(first) << 8) | u32::from(next),
                        length: 2,
                    }
                }
                Some(0xa0 | 0xff) => Decoded::Invalid { length: 2 },
                _ => Decoded::Invalid { length: 1 },
            },
            Self::Eucjp if first == 0x8f => Self::eucjp_triple(input, first, second),
            Self::Eucjp if matches!(first, 0xa0 | 0xff) => Decoded::Invalid { length: 1 },
            _ => Decoded::Valid {
                code: u32::from(first),
                length: 1,
            },
        }
    }

    /// A three-byte EUC-JP character, which starts with 0x8f.
    fn eucjp_triple(input: &[u8], first: u8, second: Option<u8>) -> Decoded {
        let trail = |byte: u8| (0xa1..=0xfe).contains(&byte);
        let third = input.get(2).copied();
        if let (Some(second), Some(third)) = (second, third)
            && trail(second)
            && trail(third)
        {
            Decoded::Valid {
                code: (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third),
                length: 3,
            }
        } else {
            let consumed = if !second.is_some_and(|byte| matches!(byte, 0xa0 | 0xff)) {
                1
            } else if !third.is_some_and(|byte| matches!(byte, 0xa0 | 0xff)) {
                2
            } else {
                3
            };
            Decoded::Invalid { length: consumed }
        }
    }
}

/// A UTF-8 character, as `php_next_utf8_char` reads it. After an invalid
/// lead byte, the bytes that cannot start a character are skipped with it,
/// up to the length the lead byte announced.
fn utf8(input: &[u8]) -> Decoded {
    let first = input[0];
    let (length, minimum, payload) = match first {
        0x00..=0x7f => {
            return Decoded::Valid {
                code: u32::from(first),
                length: 1,
            };
        }
        0xc2..=0xdf => (2, 0x80, 0x1f),
        0xe0..=0xef => (3, 0x800, 0x0f),
        0xf0..=0xf4 => (4, 0x1_0000, 0x07),
        _ => return Decoded::Invalid { length: 1 },
    };
    let continuation = |byte: &u8| byte & 0xc0 == 0x80;
    let Some(rest) = input
        .get(1..length)
        .filter(|rest| rest.iter().all(continuation))
    else {
        let could_start = |byte: &u8| *byte < 0x80 || (0xc2..=0xf4).contains(byte);
        let mut skipped = 1;
        while skipped < length && input.get(skipped).is_some_and(|byte| !could_start(byte)) {
            skipped += 1;
        }
        return Decoded::Invalid { length: skipped };
    };
    let code = rest.iter().fold(u32::from(first & payload), |code, &byte| {
        (code << 6) | u32::from(byte & 0x3f)
    });
    // Overlong forms, surrogates and values past U+10FFFF are invalid.
    if code < minimum || code > 0x10_ffff || (0xd800..=0xdfff).contains(&code) {
        Decoded::Invalid { length }
    } else {
        Decoded::Valid { code, length }
    }
}
