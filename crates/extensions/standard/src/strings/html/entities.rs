//! `html_entity_decode("&fjlig;", ENT_HTML5)` returns `"fj"`.
//!
//! PHP's named entities, ordered by name for decoding and by code point
//! for translation tables. The generator preserves PHP's choice of aliases.

use std::sync::OnceLock;

use super::options::Document;

#[derive(Clone, Copy)]
pub(super) struct Entity {
    pub(super) name: &'static str,
    pub(super) first: u32,
    pub(super) second: Option<u32>,
}

pub(super) fn code(text: &str) -> u32 {
    u32::from_str_radix(text.trim_start_matches("0x"), 16).expect("generated hexadecimal code")
}

fn rows(data: &'static str) -> Vec<Entity> {
    data.lines()
        .map(|row| {
            let mut fields = row.split('\t');
            let name = fields.next().expect("generated entity name");
            let first = code(fields.next().expect("generated code point"));
            let second = fields.next().map(code).filter(|&second| second != 0);
            Entity {
                name,
                first,
                second,
            }
        })
        .collect()
}

pub(super) fn named(name: &[u8], document: Document) -> Option<Entity> {
    static HTML4: OnceLock<Vec<Entity>> = OnceLock::new();
    static HTML5: OnceLock<Vec<Entity>> = OnceLock::new();
    let names = match document {
        Document::Html5 => HTML5.get_or_init(|| rows(include_str!("tables/html5-names.tsv"))),
        Document::Xml1 | Document::Xhtml if document == Document::Xml1 || name == b"apos" => {
            return translation(Document::Xml1, false)
                .iter()
                .find(|row| row.name.as_bytes() == name)
                .copied();
        }
        _ => HTML4.get_or_init(|| rows(include_str!("tables/html4-names.tsv"))),
    };
    names
        .binary_search_by(|row| row.name.as_bytes().cmp(name))
        .ok()
        .map(|index| names[index])
}

pub(super) fn translation(document: Document, all: bool) -> &'static [Entity] {
    static BASIC_HTML: OnceLock<Vec<Entity>> = OnceLock::new();
    static BASIC_XML: OnceLock<Vec<Entity>> = OnceLock::new();
    static HTML4: OnceLock<Vec<Entity>> = OnceLock::new();
    static HTML5: OnceLock<Vec<Entity>> = OnceLock::new();
    match (document, all) {
        (Document::Html5, true) => {
            HTML5.get_or_init(|| rows(include_str!("tables/html5-translation.tsv")))
        }
        (Document::Html401 | Document::Xhtml, true) => {
            HTML4.get_or_init(|| rows(include_str!("tables/html4-translation.tsv")))
        }
        (Document::Html401, false) => {
            BASIC_HTML.get_or_init(|| rows(include_str!("tables/be_noapos-translation.tsv")))
        }
        _ => BASIC_XML.get_or_init(|| rows(include_str!("tables/be_apos-translation.tsv"))),
    }
}
