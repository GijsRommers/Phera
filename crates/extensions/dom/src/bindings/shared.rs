//! Live document nodes exported to other XML extensions.
//!
//! ```php
//! $element = dom_import_simplexml($xml);
//! $element->setAttribute('active', 'yes');
//! echo $xml['active'];
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, NativeResult, Object};

use crate::document::{Document, ParseError};
use crate::tree::{Arena, Kind, NodeId};

use super::{Location, libxml, location};

/// A pinned arena index whose descendants remain available after DOM removes it.
#[derive(Clone)]
pub struct SharedNode {
    location: Location,
    _pin: Rc<()>,
}

impl SharedNode {
    fn new(location: Location) -> Self {
        let pin = location.shared.borrow_mut().pin(location.id);
        Self {
            location,
            _pin: pin,
        }
    }

    /// Wrap the document element of a newly parsed document.
    pub fn root(document: Document) -> Option<Self> {
        let root = document
            .arena
            .children(Arena::DOCUMENT)
            .find(|&id| document.arena[id].kind == Kind::Element)?;
        Some(Self::new(Location::new(document, true, root)))
    }

    /// Share the arena of an existing DOM object.
    pub fn from_dom(object: &Object) -> Option<Self> {
        location(object).ok().map(Self::new)
    }

    pub fn id(&self) -> NodeId {
        self.location.id
    }

    pub fn owns_document(&self) -> bool {
        self.location.shared.borrow().owned
    }

    pub fn document<T>(&self, use_document: impl FnOnce(&Document) -> T) -> T {
        use_document(&self.location.shared.borrow().document)
    }

    pub fn document_mut<T>(&self, use_document: impl FnOnce(&mut Document) -> T) -> T {
        use_document(&mut self.location.shared.borrow_mut().document)
    }

    /// A node from the same arena can be selected without copying the document.
    pub fn select(&self, node: NodeId) -> Option<Self> {
        if node == self.id() {
            return Some(self.clone());
        }
        self.document(|document| document.arena.get(node).is_some())
            .then(|| Self::new(self.location.at(node)))
    }

    /// Use DOM's weak wrapper cache when a shared node is exported back to PHP.
    pub fn export_dom(&self, context: &mut dyn Context) -> NativeResult {
        self.location.clone().wrap(context)
    }

    pub fn remove(&self, node: NodeId) {
        let mut state = self.location.shared.borrow_mut();
        if state.document.arena.get(node).is_some() {
            state.remove(node);
        }
    }

    pub fn assign_text(&self, value: String) {
        let mut state = self.location.shared.borrow_mut();
        let id = self.id();
        let kind = state.document.arena[id].kind;
        if kind == Kind::Attribute {
            state.document.arena.assign_attribute(id, value);
        } else if kind == Kind::Element {
            while let Some(child) = state.document.arena[id].first_child() {
                state.remove(child);
            }
            if !value.is_empty() {
                let arena = &mut state.document.arena;
                let text = arena.allocate(crate::tree::Node::new(
                    Kind::Text,
                    crate::tree::Name::new("#text"),
                    value,
                ));
                let _ = arena.insert(id, text, None);
            }
        } else {
            state.document.arena[id].value = value;
        }
    }
}

/// Report detailed PHP diagnostics into the request-local libxml buffer.
pub fn publish_xml_source_errors(
    context: &mut dyn Context,
    function: &str,
    source: &str,
    input: &[u8],
    diagnostics: Vec<ParseError>,
    flags: i64,
) {
    libxml::publish_with_source(context, function, source, input, diagnostics, flags);
}

pub fn publish_xml_file_error(context: &mut dyn Context, function: &str, path: &str) {
    libxml::file_failure(context, function, path);
}
