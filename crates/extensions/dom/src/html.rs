//! Legacy HTML document loading with HTML5 tokenization and tree construction.
//!
//! ```php
//! $document->loadHTML('<p>hi</p>', LIBXML_HTML_NOIMPLIED | LIBXML_HTML_NODEFDTD);
//! ```

mod sink;

use std::borrow::Cow;

use html5ever::tendril::TendrilSink;
use html5ever::{ParseOpts, parse_document};

use crate::document::{Document, Settings};
use crate::tree::{Arena, Doctype, Kind, Name, Node, NodeId};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub omit_implied: bool,
    pub omit_default_doctype: bool,
    pub strip_blanks: bool,
}

pub fn parse(bytes: &[u8], options: Options) -> Document {
    let sample = bytes.get(..bytes.len().min(4096)).unwrap_or(bytes);
    let sample = String::from_utf8_lossy(sample).to_ascii_lowercase();
    let encoding = charset(&sample);
    let is_utf8 = bytes.starts_with(b"\xef\xbb\xbf")
        || encoding
            .as_ref()
            .is_some_and(|name| name.eq_ignore_ascii_case("utf-8"));
    let input = if is_utf8 {
        String::from_utf8_lossy(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
    } else if bytes.is_ascii() {
        String::from_utf8_lossy(bytes)
    } else {
        Cow::Owned(bytes.iter().copied().map(char::from).collect())
    };
    let mut arena = parse_document(sink::Sink::default(), ParseOpts::default()).one(input.as_ref());
    let lower = input.to_ascii_lowercase();
    adjust_tree(&mut arena, &lower, options);
    Document {
        arena,
        settings: Settings {
            html: true,
            standalone: Some(true),
            encoding: encoding.filter(|_| !sample.starts_with("<?xml")),
            ..Settings::default()
        },
    }
}

fn charset(sample: &str) -> Option<String> {
    let marker = if sample.starts_with("<?xml") {
        "encoding="
    } else {
        "charset="
    };
    let tail = sample.split_once(marker)?.1.trim_start();
    let tail = tail.trim_start_matches(['"', '\'']);
    let name = tail
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_'))
        .next()?;
    (!name.is_empty()).then(|| name.to_owned())
}

fn adjust_tree(arena: &mut Arena, source: &str, options: Options) {
    let nodes: Vec<_> = arena.descendants(Arena::DOCUMENT).collect();
    let explicit_html = has_tag(source, "html");
    let explicit_head = has_tag(source, "head");
    let explicit_body = has_tag(source, "body");
    let explicit_tbody = has_tag(source, "tbody");
    let mut implied_html = None;

    for id in nodes {
        match (arena[id].kind, arena[id].name.local.as_str()) {
            (Kind::Comment, _) if arena[id].value.starts_with("?xml ") => {
                let data = arena[id].value[5..].trim_end_matches('?').to_owned();
                arena[id].kind = Kind::ProcessingInstruction;
                arena[id].name = Name::new("xml");
                arena[id].value = data;
            }
            (Kind::Element, "html") if !explicit_html && options.omit_implied => {
                implied_html = Some(id);
            }
            (Kind::Element, "head") if !explicit_head => {
                if options.omit_implied {
                    unwrap(arena, id);
                } else if arena[id].first_child().is_none() {
                    arena.detach(id);
                }
            }
            (Kind::Element, "body") => {
                wrap_initial_text(arena, id);
                if !explicit_body {
                    if options.omit_implied {
                        unwrap(arena, id);
                    } else if arena[id].first_child().is_none() {
                        arena.detach(id);
                    }
                }
            }
            (Kind::Element, "tbody") if !explicit_tbody => unwrap(arena, id),
            (Kind::Text, _)
                if options.strip_blanks
                    && arena[id].value.chars().all(char::is_whitespace)
                    && arena[id].value != " "
                    && arena[id]
                        .parent()
                        .is_some_and(|parent| arena[parent].name.local != "pre") =>
            {
                arena.detach(id);
            }
            _ => {}
        }
    }

    if let Some(html) = implied_html {
        let root = arena
            .children(html)
            .find(|&id| arena[id].kind == Kind::Element);
        if let Some(root) = root {
            let siblings: Vec<_> = arena.children(html).filter(|&id| id != root).collect();
            for sibling in siblings {
                if arena[sibling].kind == Kind::Element {
                    let _ = arena.insert(root, sibling, None);
                }
            }
        }
        unwrap(arena, html);
    }

    let doctype = arena
        .children(Arena::DOCUMENT)
        .find(|&id| arena[id].kind == Kind::Doctype);
    let doctype = if doctype.is_none() && !options.omit_default_doctype {
        let mut node = Node::new(Kind::Doctype, Name::new("html"), "");
        node.doctype = Some(Box::new(Doctype {
            public_id: "-//W3C//DTD HTML 4.0 Transitional//EN".into(),
            system_id: "http://www.w3.org/TR/REC-html40/loose.dtd".into(),
            ..Doctype::default()
        }));
        Some(arena.allocate(node))
    } else {
        doctype
    };
    if let Some(doctype) = doctype {
        let first = arena[Arena::DOCUMENT].first_child();
        let _ = arena.insert(Arena::DOCUMENT, doctype, first);
    }
}

fn wrap_initial_text(arena: &mut Arena, body: NodeId) {
    let Some(first) = arena[body].first_child() else {
        return;
    };
    if arena[first].kind != Kind::Text || arena[first].value.trim().is_empty() {
        return;
    }
    let paragraph = arena.allocate(Node::new(Kind::Element, Name::new("p"), ""));
    let _ = arena.insert(body, paragraph, Some(first));
    let children: Vec<_> = arena
        .children(body)
        .skip(1)
        .take_while(|&id| {
            arena[id].kind == Kind::Text
                || matches!(
                    arena[id].name.local.as_str(),
                    "span" | "a" | "b" | "em" | "i" | "strong"
                )
        })
        .collect();
    for child in children {
        let _ = arena.insert(paragraph, child, None);
    }
}

fn unwrap(arena: &mut Arena, id: NodeId) {
    if let Some(parent) = arena[id].parent() {
        let children: Vec<_> = arena.children(id).collect();
        for child in children {
            let _ = arena.insert(parent, child, Some(id));
        }
    }
    arena.detach(id);
}

fn has_tag(input: &str, name: &str) -> bool {
    input.match_indices('<').any(|(start, _)| {
        input[start + 1..]
            .strip_prefix(name)
            .is_some_and(|tail| tail.starts_with(['>', '/', ' ', '\t', '\r', '\n']))
    })
}
