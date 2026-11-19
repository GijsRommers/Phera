//! Resolve namespace prefixes while processing start and end tags.
//!
//! ```php
//! $document->loadXML('<a xmlns:p="urn:p"><p:b p:c="1"/></a>');
//! echo $document->documentElement->firstChild->namespaceURI;
//! ```

use std::borrow::Cow;
use std::collections::HashSet;

use quick_xml::events::BytesStart;
use quick_xml::events::attributes::AttrError;

use super::{Parser, ScopeChanges, extra_content, names};
use crate::document::ParseError;
use crate::tree::{Kind, Name, Node};

impl Parser<'_> {
    pub(super) fn element(
        &mut self,
        tag: BytesStart<'_>,
        line: usize,
        push: bool,
    ) -> Result<(), ParseError> {
        if self.open.is_empty() {
            if self.root {
                return Err(extra_content());
            }
            self.root = true;
        }
        if !self.options.huge && self.open.len() >= 256 {
            return Err(ParseError::fatal(
                1,
                "Excessive depth in document: 256 use XML_PARSE_HUGE option",
            ));
        }
        let tag_name = tag.name();
        let name = tag_name.as_ref();
        if !names::valid(name) {
            return Err(ParseError::fatal(68, "StartTag: invalid element name"));
        }

        let mut scope_changes = Vec::new();
        let mut first_attribute = None;
        let mut attributes = Vec::new();
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|error| attribute_error(error, &tag))?;
            let name = attribute.key.into_inner();
            if !names::valid(name) {
                return Err(ParseError::fatal(68, "error parsing attribute name"));
            }
            let raw = &attribute.value;
            if raw.contains('<') {
                return Err(ParseError::fatal(
                    38,
                    "Unescaped '<' not allowed in attributes values",
                ));
            }
            let value = self.entities.attribute(raw)?;
            if let Some(prefix) = namespace_declaration(name) {
                let previous = self.scope.insert(prefix.into(), value.clone());
                scope_changes.push((prefix.into(), previous));
            }
            let attribute = (Cow::Borrowed(name), value);
            if first_attribute.is_none() {
                first_attribute = Some(attribute);
            } else {
                attributes.push(attribute);
            }
        }

        if self.options.default_attributes {
            for rule in self.attribute_rules.get(name) {
                if let Some(value) = &rule.default
                    && !first_attribute
                        .iter()
                        .chain(&attributes)
                        .any(|(name, _)| *name == rule.name)
                {
                    attributes.push((
                        Cow::Owned(rule.name.clone()),
                        self.entities.attribute(value)?,
                    ));
                }
            }
        }
        let local_tag = name;
        let parent = self.parent();
        let name = self.resolve_name(name, false);
        let mut element = Node::new(Kind::Element, name, "");
        element.line = line;
        let id = self.document.arena.allocate(element);
        self.document
            .arena
            .insert(parent, id, None)
            .map_err(|_| extra_content())?;

        let mut expanded = HashSet::new();
        for (key, value) in first_attribute.into_iter().chain(attributes) {
            let name = if namespace_declaration(&key).is_some() {
                names::qualified(&key, Some(names::XMLNS.into()))
            } else {
                self.resolve_name(&key, true)
            };
            if let Some(namespace) = name.namespace.as_deref()
                && namespace != names::XMLNS
                && !expanded.insert((name.local.clone(), namespace.to_owned()))
            {
                let mut error = ParseError::fatal(
                    203,
                    format!(
                        "Namespaced Attribute {} in '{}' redefined",
                        name.local, namespace
                    ),
                );
                error.level = 2;
                self.report(error);
            }
            let is_id = (name.namespace.as_deref() == Some(names::XML) && name.local == "id")
                || self
                    .attribute_rules
                    .get(local_tag)
                    .iter()
                    .any(|rule| rule.is_id && rule.name == key);
            let mut attribute = Node::new(Kind::Attribute, name, value);
            attribute.is_id = is_id;
            let attribute = self.document.arena.attribute(attribute, true);
            self.document
                .arena
                .set_attribute(id, attribute, None)
                .map_err(|_| extra_content())?;
        }
        if push {
            self.open.push((id, scope_changes));
        } else {
            self.restore(scope_changes);
        }
        Ok(())
    }

    pub(super) fn close(&mut self) {
        if let Some((_, changes)) = self.open.pop() {
            self.restore(changes);
        }
    }

    fn restore(&mut self, changes: ScopeChanges) {
        for (prefix, previous) in changes.into_iter().rev() {
            if let Some(previous) = previous {
                self.scope.insert(prefix, previous);
            } else {
                self.scope.remove(&prefix);
            }
        }
    }

    fn resolve_name(&mut self, name: &str, attribute: bool) -> Name {
        let prefix = name.split_once(':').map(|(prefix, _)| prefix);
        let namespace = if attribute && prefix.is_none() {
            None
        } else {
            self.scope
                .get(prefix.unwrap_or(""))
                .filter(|uri| !uri.is_empty())
                .cloned()
        };
        if let Some((prefix, local)) = name.split_once(':')
            && namespace.is_none()
        {
            let mut error = ParseError::fatal(
                201,
                format!("Namespace prefix {prefix} on {local} is not defined"),
            );
            error.level = 2;
            self.report(error);
            return Name::new(name);
        }
        names::qualified(name, namespace)
    }
}

fn namespace_declaration(key: &str) -> Option<&str> {
    if key == "xmlns" {
        Some("")
    } else {
        key.strip_prefix("xmlns:")
    }
}

fn attribute_error(error: AttrError, tag: &BytesStart<'_>) -> ParseError {
    if let AttrError::Duplicated(offset, _) = error {
        let name = tag.get(offset..).unwrap_or("");
        let name = name
            .split([' ', '=', '\r', '\n', '\t'])
            .next()
            .unwrap_or("");
        return ParseError::fatal(42, format!("Attribute {name} redefined"));
    }
    ParseError::fatal(65, error.to_string())
}
