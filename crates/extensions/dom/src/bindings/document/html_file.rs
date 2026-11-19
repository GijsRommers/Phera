//! Add the legacy encoding declaration used by DOMDocument::saveHTMLFile.
//!
//! ```php
//! $document->saveHTMLFile('output.html');
//! ```

use crate::tree::{Arena, Kind, Name, Node};

use super::super::attributes;

pub(super) fn ensure_content_type(arena: &mut Arena) {
    let Some(root) = arena
        .children(Arena::DOCUMENT)
        .find(|&id| arena[id].kind == Kind::Element)
    else {
        return;
    };
    let head = if arena[root].name.local.eq_ignore_ascii_case("head") {
        Some(root)
    } else if arena[root].name.local.eq_ignore_ascii_case("html") {
        arena.children(root).find(|&id| {
            arena[id].kind == Kind::Element && arena[id].name.local.eq_ignore_ascii_case("head")
        })
    } else {
        None
    };
    let Some(head) = head else { return };
    if arena.children(head).any(|id| {
        arena[id].kind == Kind::Element
            && arena[id].name.local.eq_ignore_ascii_case("meta")
            && attributes::find(arena, id, None, "http-equiv")
                .is_some_and(|id| arena[id].value.eq_ignore_ascii_case("Content-Type"))
    }) {
        return;
    }
    let first = arena[head].first_child();
    let meta = arena.allocate(Node::new(Kind::Element, Name::new("meta"), ""));
    let _ = arena.insert(head, meta, first);
    for (name, value) in [
        ("http-equiv", "Content-Type"),
        ("content", "text/html; charset=UTF-8"),
    ] {
        let attribute = arena.attribute(Node::new(Kind::Attribute, Name::new(name), value), true);
        let _ = arena.set_attribute(meta, attribute, None);
    }
}
