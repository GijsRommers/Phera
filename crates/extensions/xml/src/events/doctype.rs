//! Register internal declarations through the shared bounded DOM entity parser.
//!
//! ```php
//! xml_parse($parser, '<!DOCTYPE r [<!ENTITY e "value">]><r>&e;</r>', true);
//! ```

use phera_ext_dom::tree::{Arena, Kind};
use phera_ext_dom::xml::{self, EntityResolver};
use phera_extension_api::Value;

use super::{Emission, Handler, Parser};

impl Parser {
    pub(super) fn doctype(&mut self, text: &str, events: &mut Vec<Emission>) -> Result<(), i64> {
        let input = format!("<!DOCTYPE {text}><phera/>");
        let parsed = xml::parse(input.as_bytes(), xml::Options::default());
        let Some(doc) = parsed.document else {
            return Err(parsed.diagnostics.last().map_or(61, |error| error.code));
        };
        let arena = &doc.arena;
        let Some(doctype) = arena
            .children(Arena::DOCUMENT)
            .find(|&id| arena[id].kind == Kind::Doctype)
        else {
            return Err(61);
        };
        self.entities = EntityResolver::from_doctype(arena, doctype);
        if self.namespace_separator.is_some()
            && let Some(details) = &arena[doctype].doctype
        {
            self.attribute_defaults
                .clone_from(&details.attribute_defaults);
        }
        for entry in arena.children(doctype) {
            let node = &arena[entry];
            let Some(details) = &node.dtd_entry else {
                continue;
            };
            if node.kind == Kind::Notation || details.notation.is_some() {
                let mut args = vec![
                    self.encode(&node.name.local),
                    Value::Bool(false),
                    self.optional(details.system_id.as_deref()),
                    self.optional(details.public_id.as_deref()),
                ];
                let handler = if node.kind == Kind::Notation {
                    Handler::Notation
                } else {
                    args.push(self.optional(details.notation.as_deref()));
                    Handler::Unparsed
                };
                events.push(self.emission(handler, args, self.cursor));
            } else if node.kind == Kind::Entity && details.system_id.is_some() {
                self.external
                    .insert(node.name.local.clone(), details.as_ref().clone());
            }
        }
        Ok(())
    }
}
