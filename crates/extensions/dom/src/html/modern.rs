//! Parse HTML5 documents and context-sensitive fragments into the shared arena.
//!
//! ```php
//! $doc = Dom\HTMLDocument::createFromString('<table><tr><td>x</td></tr></table>');
//! $doc->querySelector('tbody')->innerHTML = '<tr><td>new</td></tr>';
//! ```

use encoding_rs::{Encoding, UTF_8};
use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, QualName, parse_document, parse_fragment};

use crate::document::{Document, Settings};
use crate::tree::{Arena, Kind, Name, NodeId};

use super::{XHTML, charset, has_tag, sink::Sink, unwrap};

pub fn parse_modern(
    bytes: &[u8],
    override_encoding: Option<&'static Encoding>,
    no_implied: bool,
    omit_default_namespace: bool,
) -> Document {
    let sample = &bytes[..bytes.len().min(4096)];
    let guessed = charset(&String::from_utf8_lossy(sample).to_ascii_lowercase());
    let encoding = override_encoding
        .or_else(|| Encoding::for_bom(bytes).map(|(encoding, _)| encoding))
        .or_else(|| Encoding::for_label(guessed.as_deref()?.as_bytes()))
        .unwrap_or(UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    let mut sink = Sink::default();
    sink.modern = true;
    let (mut arena, quirks) = parse_document(sink, ParseOpts::default()).one(text.as_ref());
    if no_implied {
        let lower = text.to_ascii_lowercase();
        for local in ["head", "body", "html"] {
            if !has_tag(&lower, local) {
                let element = arena
                    .descendants(Arena::DOCUMENT)
                    .find(|&id| arena[id].kind == Kind::Element && arena[id].name.local == local);
                if let Some(element) = element {
                    unwrap(&mut arena, element);
                }
            }
        }
    }
    if omit_default_namespace {
        let html_elements: Vec<_> = arena
            .descendants(Arena::DOCUMENT)
            .filter(|&id| arena[id].name.namespace.as_deref() == Some(XHTML))
            .collect();
        for id in html_elements {
            arena[id].name.namespace = None;
        }
    }
    Document {
        arena,
        settings: Settings {
            html: true,
            modern: true,
            quirks,
            standalone: Some(true),
            encoding: Some(encoding.name().into()),
            uri: Some("about:blank".into()),
            ..Settings::default()
        },
    }
}

pub fn fragment(input: &str, parent: &Name) -> (Arena, NodeId) {
    let name = QualName::new(
        parent.prefix.as_deref().map(Into::into),
        parent.namespace.as_deref().unwrap_or(XHTML).into(),
        parent.local.as_str().into(),
    );
    let mut sink = Sink::default();
    sink.modern = true;
    let (arena, _) = parse_fragment(sink, ParseOpts::default(), name, Vec::new(), false).one(input);
    let root = arena[Arena::DOCUMENT]
        .first_child()
        .unwrap_or(Arena::DOCUMENT);
    (arena, root)
}
