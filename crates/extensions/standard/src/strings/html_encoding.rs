//! PHP's supported HTML encodings and multibyte error-consumption rules.

use phera_extension_api::Context;

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
    /// Unsupported names warn and fall back to the default UTF-8 charset.
    pub(super) fn resolve(context: &mut dyn Context, name: &[u8]) -> Self {
        let end = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        let name = name.get(..end).unwrap_or_default();
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

    /// Non-UTF-8 encodings use a numeric entity for the replacement character.
    pub(super) fn replacement(&self) -> &'static [u8] {
        if matches!(self, Self::Utf8) {
            b"\xef\xbf\xbd"
        } else {
            b"&#xFFFD;"
        }
    }

    /// For partially supported Asian encodings, only ASCII controls are validated.
    pub(super) fn disallowed(&self, code: u32, document: super::html::Document) -> bool {
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
            !super::html::allowed(code, document)
        }
    }

    /// Invalid continuation bytes may be retained for the next decoding step.
    pub(super) fn next(&self, input: &[u8]) -> (u32, usize, bool) {
        let first = input.first().copied().unwrap_or_default();
        let second = input.get(1).copied();
        let trail = |byte: u8| (0xa1..=0xfe).contains(&byte);
        match self {
            Self::Utf8 => super::html::utf8_char(input),
            Self::Latin1 | Self::SingleByte => (u32::from(first), 1, true),
            Self::Big5 | Self::Big5Hkscs if (0x81..=0xfe).contains(&first) => match second {
                Some(next) if (0x40..=0x7e).contains(&next) || trail(next) => {
                    ((u32::from(first) << 8) | u32::from(next), 2, true)
                }
                Some(next) if matches!(self, Self::Big5Hkscs) && matches!(next, 0x80 | 0xff) => {
                    (0, 2, false)
                }
                _ => (0, 1, false),
            },
            Self::Gb2312 if trail(first) => match second {
                Some(next) if trail(next) => ((u32::from(first) << 8) | u32::from(next), 2, true),
                Some(0x8e | 0x8f | 0xa0 | 0xff) => (0, 2, false),
                _ => (0, 1, false),
            },
            Self::Gb2312 if matches!(first, 0x8e | 0x8f | 0xa0 | 0xff) => (0, 1, false),
            Self::Sjis if (0x81..=0x9f).contains(&first) || (0xe0..=0xfc).contains(&first) => {
                match second {
                    Some(next) if (0x40..0xfd).contains(&next) && next != 0x7f => {
                        ((u32::from(first) << 8) | u32::from(next), 2, true)
                    }
                    Some(0x80 | 0xa0 | 0xfd..=0xff) => (0, 2, false),
                    _ => (0, 1, false),
                }
            }
            Self::Sjis if first >= 0x80 && !(0xa1..=0xdf).contains(&first) => (0, 1, false),
            Self::Eucjp if trail(first) || first == 0x8e => match second {
                Some(next)
                    if (if first == 0x8e {
                        (0xa1..=0xdf).contains(&next)
                    } else {
                        trail(next)
                    }) =>
                {
                    ((u32::from(first) << 8) | u32::from(next), 2, true)
                }
                Some(0xa0 | 0xff) => (0, 2, false),
                _ => (0, 1, false),
            },
            Self::Eucjp if first == 0x8f => Self::eucjp_triple(input, first, second),
            Self::Eucjp if matches!(first, 0xa0 | 0xff) => (0, 1, false),
            _ => (u32::from(first), 1, true),
        }
    }

    /// PHP consumes invalid EUC-JP prefixes according to the first bad continuation.
    fn eucjp_triple(input: &[u8], first: u8, second: Option<u8>) -> (u32, usize, bool) {
        let trail = |byte: u8| (0xa1..=0xfe).contains(&byte);
        let third = input.get(2).copied();
        if let (Some(second), Some(third)) = (second, third)
            && trail(second)
            && trail(third)
        {
            (
                (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third),
                3,
                true,
            )
        } else {
            let consumed = if !second.is_some_and(|byte| matches!(byte, 0xa0 | 0xff)) {
                1
            } else if !third.is_some_and(|byte| matches!(byte, 0xa0 | 0xff)) {
                2
            } else {
                3
            };
            (0, consumed, false)
        }
    }
}
