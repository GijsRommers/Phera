//! ICU encoding aliases and single-byte tables used by UConverter.
//!
//! ```php
//! UConverter::transcode("\x80", 'UTF-8', 'windows-1252'); // "€"
//! ```

use std::sync::OnceLock;

use icu_properties::{CodePointSetData, props::DefaultIgnorableCodePoint};
use phera_ext_mbstring::unicode_codec::UnicodeCodec;

const REPLACEMENT: u32 = 0xfffd;

pub(super) struct Encoding {
    pub name: &'static str,
    pub kind: i64,
    pub substitution: Vec<u8>,
    pub aliases: Vec<(&'static str, bool)>,
    table: Option<&'static [u8]>,
}

fn catalog() -> &'static [Encoding] {
    static CATALOG: OnceLock<Vec<Encoding>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut encodings = Vec::<Encoding>::new();
        let mappings = include_bytes!("../../data/converter-single-byte.bin");
        for row in include_str!("../../data/converter-encodings.tsv").lines() {
            if let Some(alias) = row.strip_prefix('\t') {
                if let Some(encoding) = encodings.last_mut() {
                    let label = alias.strip_prefix('!');
                    encoding
                        .aliases
                        .push((label.unwrap_or(alias), label.is_some()));
                }
                continue;
            }
            let mut fields = row.split('\t');
            let (Some(name), Some(kind), Some(subst), Some(table)) =
                (fields.next(), fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let Ok(kind) = kind.parse() else {
                continue;
            };
            let substitution = subst
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .filter_map(|bytes| u8::from_str_radix(std::str::from_utf8(bytes).ok()?, 16).ok())
                .collect();
            let table = table
                .parse::<usize>()
                .ok()
                .and_then(|index| mappings.get(index * 1024..(index + 1) * 1024));
            encodings.push(Encoding {
                name,
                kind,
                substitution,
                aliases: Vec::new(),
                table,
            });
        }
        encodings
    })
}

pub(super) fn find(name: &[u8]) -> Option<(&'static Encoding, bool)> {
    let name = name.split(|byte| *byte == 0).next().unwrap_or_default();
    let key: Vec<_> = alias_key(name).collect();
    catalog().iter().find_map(|encoding| {
        encoding.aliases.iter().find_map(|&(alias, ambiguous)| {
            alias_key(alias.as_bytes())
                .eq(key.iter().copied())
                .then_some((encoding, ambiguous))
        })
    })
}

fn alias_key(name: &[u8]) -> impl Iterator<Item = u8> {
    name.iter()
        .filter(|byte| byte.is_ascii_alphanumeric())
        .map(u8::to_ascii_lowercase)
}

impl Encoding {
    pub fn valid_substitution(&self, bytes: &[u8]) -> bool {
        match self.kind {
            4 => (1..=3).contains(&bytes.len()),
            5 | 6 | 29 => bytes.len() == 2,
            7 | 8 | 30 => bytes.len() == 4,
            _ => bytes.len() == 1,
        }
    }

    fn decode(&self, bytes: &[u8]) -> Vec<u32> {
        if let Some(table) = self.table {
            let rows = table.as_chunks::<4>().0;
            bytes
                .iter()
                .map(|&byte| {
                    rows.get(usize::from(byte))
                        .map_or(REPLACEMENT, |&point| u32::from_le_bytes(point))
                })
                .collect()
        } else {
            UnicodeCodec::find(self.name).map_or_else(Vec::new, |codec| codec.decode(bytes))
        }
    }

    fn encode(&self, points: &[u32], substitution: &[u8]) -> Vec<u8> {
        if let Some(table) = self.table {
            let rows = table.as_chunks::<4>().0;
            let mut output = Vec::with_capacity(points.len());
            let ignorable = CodePointSetData::new::<DefaultIgnorableCodePoint>();
            for &point in points {
                if point == REPLACEMENT {
                    output.extend_from_slice(substitution);
                } else if let Some(index) =
                    rows.iter().position(|entry| *entry == point.to_le_bytes())
                {
                    output.push(index as u8);
                } else if !ignorable.contains32(point) {
                    output.extend_from_slice(substitution);
                }
            }
            return output;
        }
        if points.is_empty() {
            return Vec::new();
        }
        let (name, bom): (&str, &[u8]) = match (self.name, cfg!(target_endian = "little")) {
            ("UTF-16", true) => ("UTF-16LE", &[0xff, 0xfe]),
            ("UTF-16", false) => ("UTF-16BE", &[0xfe, 0xff]),
            ("UTF-32", true) => ("UTF-32LE", &[0xff, 0xfe, 0, 0]),
            ("UTF-32", false) => ("UTF-32BE", &[0, 0, 0xfe, 0xff]),
            (name, _) => (name, &[]),
        };
        let mut output = bom.to_vec();
        if let Some(codec) = UnicodeCodec::find(name) {
            output.extend(codec.encode(points));
        }
        output
    }
}

pub(super) fn convert(
    bytes: &[u8],
    source: &Encoding,
    destination: &Encoding,
    substitution: &[u8],
) -> Vec<u8> {
    let mut points = source.decode(bytes);
    for point in &mut points {
        if char::from_u32(*point).is_none() {
            *point = REPLACEMENT;
        }
    }
    destination.encode(&points, substitution)
}
