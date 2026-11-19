//! Run CSS selectors on arena elements in document order.
//!
//! ```php
//! $links = $document->querySelectorAll('nav > a[href]:not(.hidden)');
//! ```

mod matching;
mod syntax;

use cssparser::Parser;
use phera_extension_api::{Context, NativeResult, Value, args};
use selectors::context::{
    MatchingContext, MatchingForInvalidation, MatchingMode, NeedsSelectorFlags, QuirksMode,
    SelectorCaches,
};
use selectors::matching::matches_selector_list;
use selectors::parser::ParseRelative;
use selectors::{OpaqueElement, SelectorList};

use crate::tree::{Kind, NodeId};

use super::super::{Location, This, collection, dom_error, this_node};
use matching::Element;
use syntax::Css;

enum Search {
    First,
    All,
    SelfOnly,
    Ancestors,
}

pub(super) fn first(c: &mut dyn Context, t: This<'_>, a: &[Value]) -> NativeResult {
    query(c, t, a, Search::First)
}

pub(super) fn all(c: &mut dyn Context, t: This<'_>, a: &[Value]) -> NativeResult {
    query(c, t, a, Search::All)
}

pub(super) fn matches(c: &mut dyn Context, t: This<'_>, a: &[Value]) -> NativeResult {
    query(c, t, a, Search::SelfOnly)
}

pub(super) fn closest(c: &mut dyn Context, t: This<'_>, a: &[Value]) -> NativeResult {
    query(c, t, a, Search::Ancestors)
}

fn query(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    search: Search,
) -> NativeResult {
    let source = String::from_utf8_lossy(args::bytes(arguments, 0));
    let root = this_node(this)?;
    let found = find(&root, &source, &search)?;
    match search {
        Search::All => collection::snapshot(context, root, found),
        Search::SelfOnly => Ok(Value::Bool(!found.is_empty())),
        _ => root.wrap_optional(context, found.first().copied()),
    }
}

fn find(
    root: &Location,
    source: &str,
    search: &Search,
) -> Result<Vec<NodeId>, phera_extension_api::NativeError> {
    if source.trim().eq_ignore_ascii_case(":scope") {
        return Err(dom_error(
            12,
            "Invalid selector (Selectors. Not supported: scope)",
        ));
    }
    if !syntax::bounded(source) {
        return Err(dom_error(
            12,
            "Invalid selector (Selectors. Selector is too complex)",
        ));
    }
    let state = root.shared.borrow();
    let arena = &state.document.arena;
    let parser = syntax::Namespaces::new(arena, root.id);
    let mut input = Parser::new(source);
    let selectors: SelectorList<Css> = SelectorList::parse(&parser, &mut input, ParseRelative::No)
        .map_err(|error| syntax::error(source, &error))?;
    let mut caches = SelectorCaches::default();
    let quirks = if state.document.settings.quirks {
        QuirksMode::Quirks
    } else {
        QuirksMode::NoQuirks
    };
    let mut context = MatchingContext::new(
        MatchingMode::Normal,
        None,
        &mut caches,
        quirks,
        NeedsSelectorFlags::No,
        MatchingForInvalidation::No,
    );
    let is_element = arena[root.id].kind == Kind::Element;
    let scope = if is_element {
        Some(root.id)
    } else {
        arena
            .children(root.id)
            .find(|&id| arena[id].kind == Kind::Element)
    };
    context.scope_element = scope.map(|id| OpaqueElement::new(&arena[id]));
    let mut matches = |id| {
        if arena[id].kind != Kind::Element {
            return false;
        }
        let element = Element {
            arena,
            id,
            html: state.document.settings.html,
        };
        matches_selector_list(&selectors, &element, &mut context)
    };
    let selected = match search {
        Search::All => arena
            .content_descendants(root.id)
            .filter(|&id| matches(id))
            .collect(),
        Search::First => arena
            .content_descendants(root.id)
            .find(|&id| matches(id))
            .into_iter()
            .collect(),
        Search::SelfOnly => matches(root.id).then_some(root.id).into_iter().collect(),
        Search::Ancestors => std::iter::once(root.id)
            .chain(arena.ancestors(root.id))
            .find(|&id| matches(id))
            .into_iter()
            .collect(),
    };
    Ok(selected)
}
