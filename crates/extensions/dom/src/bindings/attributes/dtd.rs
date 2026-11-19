//! Read virtual DTD default attributes without adding them to the element.
//!
//! ```php
//! $document->loadXML('<!DOCTYPE r [<!ATTLIST r a CDATA "d">]><r/>');
//! echo $document->documentElement->getAttribute('a');
//! ```

use crate::tree::{Arena, NodeId};
use crate::xml::names::XML;

use super::super::node::namespace_scope;

pub(super) fn default_value<'a>(
    arena: &'a Arena,
    id: NodeId,
    namespace: Option<Option<&str>>,
    name: &str,
) -> Option<&'a str> {
    let doctype = arena
        .children(Arena::DOCUMENT)
        .find_map(|id| arena[id].doctype.as_deref())?;
    let attributes = doctype
        .attribute_defaults
        .get(&arena[id].name.qualified())?;
    attributes.iter().find_map(|(qualified, value)| {
        let matches = match namespace {
            None => qualified == name,
            Some(namespace) => match qualified.split_once(':') {
                Some((prefix, local)) => {
                    local == name
                        && namespace.is_some_and(|uri| {
                            if prefix == "xml" {
                                uri == XML
                            } else {
                                namespace_scope(arena, id)
                                    .find(|(bound, _)| bound == prefix)
                                    .is_some_and(|(_, bound)| bound == uri)
                            }
                        })
                }
                None => namespace.is_none() && qualified == name,
            },
        };
        matches.then_some(value.as_str())
    })
}
