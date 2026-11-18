//! `html_entity_decode("&euro;", ENT_QUOTES, "CP1252")` returns byte 0x80.
//!
//! Single-byte charset maps for PHP's HTML entity functions.

use std::sync::OnceLock;

use super::entities::code;

#[derive(Clone, Copy)]
pub(super) enum Legacy {
    Windows1252,
    Latin9,
    Windows1251,
    Iso88595,
    Cp866,
    MacRoman,
    Koi8r,
}

struct Map {
    to_unicode: [u32; 256],
    from_unicode: Vec<(u32, u8)>,
}

impl Map {
    fn parse(to: &str, from: &str) -> Self {
        let mut to_unicode = [0; 256];
        for (slot, point) in to_unicode.iter_mut().zip(to.lines()) {
            *slot = code(point);
        }
        let from_unicode = from
            .lines()
            .map(|row| {
                let (point, byte) = row.split_once('\t').expect("generated encoding pair");
                (code(point), code(byte) as u8)
            })
            .collect();
        Self {
            to_unicode,
            from_unicode,
        }
    }
}

impl Legacy {
    fn map(self) -> &'static Map {
        static WIN1252: OnceLock<Map> = OnceLock::new();
        static LATIN9: OnceLock<Map> = OnceLock::new();
        static WIN1251: OnceLock<Map> = OnceLock::new();
        static ISO88595: OnceLock<Map> = OnceLock::new();
        static CP866: OnceLock<Map> = OnceLock::new();
        static MACROMAN: OnceLock<Map> = OnceLock::new();
        static KOI8R: OnceLock<Map> = OnceLock::new();
        let (cache, to, from) = match self {
            Self::Windows1252 => (
                &WIN1252,
                include_str!("tables/win1252-to-unicode.tsv"),
                include_str!("tables/win1252-from-unicode.tsv"),
            ),
            Self::Latin9 => (
                &LATIN9,
                include_str!("tables/iso885915-to-unicode.tsv"),
                include_str!("tables/iso885915-from-unicode.tsv"),
            ),
            Self::Windows1251 => (
                &WIN1251,
                include_str!("tables/win1251-to-unicode.tsv"),
                include_str!("tables/win1251-from-unicode.tsv"),
            ),
            Self::Iso88595 => (
                &ISO88595,
                include_str!("tables/iso88595-to-unicode.tsv"),
                "",
            ),
            Self::Cp866 => (
                &CP866,
                include_str!("tables/cp866-to-unicode.tsv"),
                include_str!("tables/cp866-from-unicode.tsv"),
            ),
            Self::MacRoman => (
                &MACROMAN,
                include_str!("tables/macroman-to-unicode.tsv"),
                include_str!("tables/macroman-from-unicode.tsv"),
            ),
            Self::Koi8r => (
                &KOI8R,
                include_str!("tables/koi8r-to-unicode.tsv"),
                include_str!("tables/koi8r-from-unicode.tsv"),
            ),
        };
        cache.get_or_init(|| Map::parse(to, from))
    }

    pub(super) fn to_unicode(self, byte: u8) -> u32 {
        self.map().to_unicode[usize::from(byte)]
    }

    pub(super) fn encode(self, point: u32) -> Option<u8> {
        match self {
            Self::Iso88595 => {
                return match point {
                    0..=0xa0 | 0xad => Some(point as u8),
                    0x2116 => Some(0xf0),
                    0xa7 => Some(0xfd),
                    0x401..=0x45f if !matches!(point, 0x40d | 0x450 | 0x45d) => {
                        Some((point - 0x360) as u8)
                    }
                    _ => None,
                };
            }
            Self::Latin9 if point < 0xa4 || (0xbf..=0xff).contains(&point) => {
                return Some(point as u8);
            }
            Self::Windows1252 if (0xa0..=0xff).contains(&point) => return Some(point as u8),
            Self::MacRoman if point == 0x7f => return None,
            _ if point <= 0x7f => return Some(point as u8),
            _ => {}
        }
        let rows = &self.map().from_unicode;
        rows.binary_search_by_key(&point, |&(code_point, _)| code_point)
            .ok()
            .map(|index| rows[index].1)
    }
}
