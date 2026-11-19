//! Adapt arena nodes to the Servo selector matcher without copying a document.
//!
//! ```php
//! $document->querySelectorAll('ol > li:nth-child(odd)');
//! ```

use selectors::OpaqueElement;
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::matching::{ElementSelectorFlags, MatchingContext};

use crate::html::XHTML;
use crate::tree::{Arena, Kind, NodeId};

use super::syntax::{Atom, Css, NoPseudoElement, Pseudo};

#[derive(Clone, Copy, Debug)]
pub(super) struct Element<'a> {
    pub arena: &'a Arena,
    pub id: NodeId,
    pub html: bool,
}

impl<'a> Element<'a> {
    fn with_id(self, id: NodeId) -> Self {
        Self { id, ..self }
    }

    fn sibling(self, next: impl Fn(NodeId) -> Option<NodeId>) -> Option<Self> {
        std::iter::successors(next(self.id), |&id| next(id))
            .find(|&id| self.arena[id].kind == Kind::Element)
            .map(|id| self.with_id(id))
    }

    fn attribute(self, name: &str, namespace: Option<&str>) -> Option<&'a str> {
        self.arena[self.id].attributes().iter().find_map(|&id| {
            let attr = &self.arena[id];
            (attr.name.local == name && attr.name.namespace.as_deref() == namespace)
                .then_some(attr.value.as_str())
        })
    }

    fn has(self, name: &str) -> bool {
        self.attribute(name, None).is_some()
    }
}

impl selectors::Element for Element<'_> {
    type Impl = Css;

    fn opaque(&self) -> OpaqueElement {
        OpaqueElement::new(&self.arena[self.id])
    }
    fn parent_element(&self) -> Option<Self> {
        self.arena[self.id]
            .parent()
            .filter(|&id| self.arena[id].kind == Kind::Element)
            .map(|id| self.with_id(id))
    }
    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }
    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }
    fn is_pseudo_element(&self) -> bool {
        false
    }
    fn prev_sibling_element(&self) -> Option<Self> {
        self.sibling(|id| self.arena[id].previous_sibling())
    }
    fn next_sibling_element(&self) -> Option<Self> {
        self.sibling(|id| self.arena[id].next_sibling())
    }
    fn first_element_child(&self) -> Option<Self> {
        self.arena
            .children(self.id)
            .find(|&id| self.arena[id].kind == Kind::Element)
            .map(|id| self.with_id(id))
    }
    fn is_html_element_in_html_document(&self) -> bool {
        self.html && self.has_namespace(XHTML)
    }
    fn has_local_name(&self, local_name: &str) -> bool {
        self.arena[self.id].name.local == local_name
    }
    fn has_namespace(&self, namespace: &str) -> bool {
        self.arena[self.id]
            .name
            .namespace
            .as_deref()
            .unwrap_or_default()
            == namespace
    }
    fn is_same_type(&self, other: &Self) -> bool {
        let left = &self.arena[self.id].name;
        let right = &other.arena[other.id].name;
        left.local == right.local && left.namespace == right.namespace
    }
    fn attr_matches(
        &self,
        namespace: &NamespaceConstraint<&Atom>,
        name: &Atom,
        operation: &AttrSelectorOperation<&Atom>,
    ) -> bool {
        self.arena[self.id].attributes().iter().any(|&id| {
            let attr = &self.arena[id];
            attr.name.local == name.0
                && match namespace {
                    NamespaceConstraint::Any => true,
                    NamespaceConstraint::Specific(ns) => {
                        attr.name.namespace.as_deref().unwrap_or_default() == ns.0
                    }
                }
                && operation.eval_str(&attr.value)
        })
    }
    fn match_non_ts_pseudo_class(&self, pseudo: &Pseudo, _: &mut MatchingContext<Css>) -> bool {
        let name = self.arena[self.id].name.local.as_str();
        let form_control = matches!(
            name,
            "input" | "select" | "textarea" | "button" | "option" | "optgroup" | "fieldset"
        );
        match pseudo.0.as_str() {
            "link" | "any-link" => self.is_link(),
            "checked" => {
                (name == "input" && self.has("checked"))
                    || (name == "option" && self.has("selected"))
            }
            "enabled" => form_control && !self.has("disabled"),
            "disabled" => form_control && self.has("disabled"),
            "required" => form_control && self.has("required"),
            "optional" => form_control && !self.has("required"),
            "read-write" => {
                self.has("contenteditable")
                    || (matches!(name, "input" | "textarea")
                        && !self.has("readonly")
                        && !self.has("disabled"))
            }
            "read-only" => {
                !self.has("contenteditable")
                    && (!matches!(name, "input" | "textarea")
                        || self.has("readonly")
                        || self.has("disabled"))
            }
            "placeholder-shown" => {
                matches!(name, "input" | "textarea")
                    && self.has("placeholder")
                    && self.attribute("value", None).is_none_or(str::is_empty)
            }
            _ => false,
        }
    }
    fn match_pseudo_element(&self, pseudo: &NoPseudoElement, _: &mut MatchingContext<Css>) -> bool {
        match *pseudo {}
    }
    fn apply_selector_flags(&self, _: ElementSelectorFlags) {}
    fn is_link(&self) -> bool {
        matches!(
            self.arena[self.id].name.local.as_str(),
            "a" | "area" | "link"
        ) && self.has("href")
    }
    fn is_html_slot_element(&self) -> bool {
        false
    }
    fn has_id(&self, id: &Atom, case: CaseSensitivity) -> bool {
        self.attribute("id", None)
            .is_some_and(|value| case.eq(value.as_bytes(), id.0.as_bytes()))
    }
    fn has_class(&self, name: &Atom, case: CaseSensitivity) -> bool {
        self.attribute("class", None).is_some_and(|value| {
            value
                .split_ascii_whitespace()
                .any(|class| case.eq(class.as_bytes(), name.0.as_bytes()))
        })
    }
    fn has_custom_state(&self, _: &Atom) -> bool {
        false
    }
    fn imported_part(&self, _: &Atom) -> Option<Atom> {
        None
    }
    fn is_part(&self, _: &Atom) -> bool {
        false
    }
    fn is_empty(&self) -> bool {
        self.arena
            .children(self.id)
            .all(|id| match self.arena[id].kind {
                Kind::Element => false,
                Kind::Text | Kind::Cdata => self.arena[id].value.is_empty(),
                _ => true,
            })
    }
    fn is_root(&self) -> bool {
        self.arena[self.id].parent() == Some(Arena::DOCUMENT)
    }
    fn add_element_unique_hashes(&self, _: &mut BloomFilter) -> bool {
        false
    }
}
