//! Reparse bounded entity replacement markup inside the current namespace scope.
//!
//! ```php
//! $document->loadXML('<!DOCTYPE r [<!ENTITY e "<a/>">]><r>&e;</r>', LIBXML_NOENT);
//! ```

use std::fmt::Write;

use crate::document::ParseError;
use crate::tree::{Arena, Kind};

use super::{Options, Parser, entities::predefined, parse};

impl Parser<'_> {
    pub(super) fn reference(&mut self, name: &str, line: usize) -> Result<(), ParseError> {
        if let Some(value) = predefined(name) {
            return self.text(value, Kind::Text);
        }
        let (value, general) = match self.entities.replacement(name) {
            Ok(result) => result,
            Err(error) if error.code == 26 => {
                self.report(error);
                return Ok(());
            }
            Err(error) => return Err(error),
        };
        if !general {
            return self.text(&value, Kind::Text);
        }
        if !value.contains(['<', '&', '\r']) {
            if self.options.substitute_entities {
                return self.text(&value, Kind::Text);
            }
            self.document.arena.materialize_entity(name, &value);
            self.add(Kind::EntityReference, name, &value, line)?;
            return Ok(());
        }
        let mut wrapper = String::from("<entity-replacement");
        for (prefix, uri) in &self.scope {
            if prefix == "xml" {
                continue;
            }
            wrapper.push_str(" xmlns");
            if !prefix.is_empty() {
                let _ = write!(wrapper, ":{prefix}");
            }
            wrapper.push_str("=\"");
            for character in uri.chars() {
                match character {
                    '<' => wrapper.push_str("&lt;"),
                    '&' => wrapper.push_str("&amp;"),
                    '"' => wrapper.push_str("&quot;"),
                    _ => wrapper.push(character),
                }
            }
            wrapper.push('"');
        }
        wrapper.push('>');
        wrapper.push_str(&value);
        wrapper.push_str("</entity-replacement>");
        // The expansion has no entity declarations. The synthetic element also
        // prevents a replacement from supplying a doctype or XML declaration.
        let parsed = parse(
            wrapper.as_bytes(),
            Options {
                huge: self.options.huge,
                ..Options::default()
            },
        );
        let Some(replacement) = parsed.document else {
            for error in parsed.diagnostics {
                self.report(ParseError::fatal(error.code, error.message.trim_end()));
            }
            return Err(ParseError::fatal(
                26,
                format!("Entity '{name}' failed to parse"),
            ));
        };
        let source = &replacement.arena;
        let Some(root) = source[Arena::DOCUMENT].first_child() else {
            return Ok(());
        };
        if self.options.substitute_entities {
            let parent = self.parent();
            for child in source.children(root) {
                if source[child].kind == Kind::Text {
                    self.text(&source[child].value, Kind::Text)?;
                } else {
                    let arena = &mut self.document.arena;
                    let copy = arena.import(source, child, true);
                    arena
                        .insert(parent, copy, None)
                        .map_err(|_| self.unexpected_text())?;
                }
            }
        } else {
            let mut text = String::new();
            for child in source.content_descendants(root) {
                if matches!(source[child].kind, Kind::Text | Kind::Cdata) {
                    text.push_str(&source[child].value);
                }
            }
            self.document
                .arena
                .materialize_entity_children(name, source, root);
            self.add(Kind::EntityReference, name, &text, line)?;
        }
        Ok(())
    }
}
