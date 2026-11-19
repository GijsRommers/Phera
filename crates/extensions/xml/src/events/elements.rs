//! Expand namespace names and attributes before invoking an element handler.
//!
//! ```php
//! xml_parse(xml_parser_create_ns(null, '|'), '<a:r xmlns:a="urn:feed"/>', true);
//! ```

use phera_extension_api::{ArrayKey, PhpArray, Value};
use quick_xml::events::BytesStart;

use super::{Emission, Handler, Open, Parser, Position};
use phera_ext_dom::xml;

impl Parser {
    pub(super) fn open(
        &mut self,
        tag: &BytesStart<'_>,
        position: Position,
        events: &mut Vec<Emission>,
    ) -> Result<(), i64> {
        if self.elements.is_empty() {
            if self.root {
                return Err(5);
            }
            self.root = true;
        }
        if self.elements.len() >= if self.options.huge { 65_536 } else { 2_048 } {
            return Err(1);
        }
        let name = tag.name();
        let name = name.as_ref();
        let name_limit = if self.options.huge {
            10_000_000
        } else {
            50_000
        };
        if name.len() > name_limit || !xml::valid_name(name) {
            return Err(68);
        }
        let qualified = name.to_owned();
        let mut attributes = Vec::new();
        let mut namespaces = Vec::new();
        for attribute in tag.attributes() {
            let attribute = attribute.map_err(|error| match error {
                quick_xml::events::attributes::AttrError::Duplicated(..) => 42i64,
                _ => 73,
            })?;
            let name = attribute.key.as_ref().to_owned();
            let value = self
                .entities
                .attribute(&attribute.value)
                .map_err(|error| error.code)?;
            let declaration = if name == "xmlns" {
                Some("")
            } else {
                name.strip_prefix("xmlns:")
            };
            if self.namespace_separator.is_some()
                && let Some(prefix) = declaration
            {
                let previous = self.namespaces.insert(prefix.to_owned(), value.clone());
                namespaces.push((prefix.to_owned(), previous));
                let data = vec![
                    if prefix.is_empty() {
                        Value::Bool(false)
                    } else {
                        self.encode(prefix)
                    },
                    self.encode(&value),
                ];
                events.push(self.emission(Handler::Namespace, data, position));
            } else {
                attributes.push((name, value));
            }
        }
        let name = self.scoped(&qualified, false);
        let mut values = PhpArray::new();
        for (key, value) in attributes {
            let name = self.scoped(&key, true);
            values.insert(
                ArrayKey::str(self.options.target.encode(&name)),
                self.encode(&value),
            );
        }
        events.push(self.emission(
            Handler::Start,
            vec![self.encode(&name), Value::from(values)],
            position,
        ));
        self.elements.push(Open {
            qualified,
            scoped: name,
            namespaces,
        });
        Ok(())
    }

    pub(super) fn close(&mut self, qualified: &str, events: &mut Vec<Emission>) -> Result<(), i64> {
        let element = self.elements.pop().ok_or(76i64)?;
        if element.qualified != qualified {
            return Err(76);
        }
        events.push(self.emission(
            Handler::End,
            vec![self.encode(&element.scoped)],
            self.cursor,
        ));
        for (prefix, previous) in element.namespaces.into_iter().rev() {
            if let Some(previous) = previous {
                self.namespaces.insert(prefix, previous);
            } else {
                self.namespaces.remove(&prefix);
            }
        }
        Ok(())
    }

    fn scoped(&self, name: &str, attribute: bool) -> String {
        let mut rendered = name.to_owned();
        if let Some(separator) = &self.namespace_separator {
            let (prefix, local) = name.split_once(':').unwrap_or(("", name));
            if !(attribute && prefix.is_empty())
                && let Some(uri) = self.namespaces.get(prefix).filter(|uri| !uri.is_empty())
            {
                rendered = format!("{uri}{separator}{local}");
            }
        }
        if self.options.case_fold {
            rendered.make_ascii_uppercase();
        }
        if attribute {
            rendered
        } else {
            rendered.chars().skip(self.options.skip_tag_start).collect()
        }
    }
}
