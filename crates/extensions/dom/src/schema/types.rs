//! Check element and attribute values against the selected XSD types.
//!
//! ```php
//! $document->schemaValidateSource($schema); // e.g. rejects non-integer quantity
//! ```

mod attributes;
mod values;

use attributes::{invalid_attribute, metadata};
use values::xml_space;

use crate::tree::{Arena, Kind, NodeId};

use super::content::Mismatch;
use super::{Definition, QName, Rule, Schema, Validation, XSD, element_error, elements};

enum Type {
    Builtin(String),
    Declared(Rule),
}

impl Schema {
    pub(super) fn validate_element(
        &self,
        arena: &Arena,
        node: NodeId,
        element: Rule,
        state: &mut Validation<'_>,
    ) {
        if self.children(element).any(|rule| {
            matches!(
                self.node(rule).name.local.as_str(),
                "key" | "keyref" | "unique"
            )
        }) {
            state.diagnostics.push(element_error(
                arena,
                node,
                1871,
                "This schema uses unsupported identity constraints.",
            ));
            return;
        }
        let Some(ty) = self.type_of(element, "anyType") else {
            state.diagnostics.push(element_error(
                arena,
                node,
                1871,
                "The declared type was not found.",
            ));
            return;
        };
        if matches!(&ty, Type::Builtin(name) if name == "anyType") {
            return;
        }
        if let Type::Declared(rule) = ty
            && self.is(rule, "complexType")
        {
            self.validate_complex(arena, node, rule, state);
        } else {
            self.validate_text(arena, node, ty, state);
            for &attribute in arena[node].attributes() {
                if !metadata(&arena[attribute].name) {
                    invalid_attribute(arena, node, &arena[attribute].name, state);
                }
            }
        }
    }

    fn type_of(&self, rule: Rule, default: &str) -> Option<Type> {
        if let Some(name) = self.attr(rule, "type") {
            return self.resolve_type(self.qname(rule, name));
        }
        self.children(rule)
            .find(|&child| self.is(child, "simpleType") || self.is(child, "complexType"))
            .map(Type::Declared)
            .or_else(|| Some(Type::Builtin(default.into())))
    }

    fn resolve_type(&self, name: QName) -> Option<Type> {
        if name.0 == XSD {
            Some(Type::Builtin(name.1))
        } else {
            self.named(Definition::Type, name).map(Type::Declared)
        }
    }

    fn validate_complex(
        &self,
        arena: &Arena,
        node: NodeId,
        rule: Rule,
        state: &mut Validation<'_>,
    ) {
        let mut attribute_owner = rule;
        if let Some(content) = self
            .children(rule)
            .find(|&child| self.is(child, "simpleContent"))
        {
            let Some(extension) = self
                .children(content)
                .find(|&child| self.is(child, "extension"))
            else {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "This simple content derivation is unsupported.",
                ));
                return;
            };
            let ty = self
                .attr(extension, "base")
                .and_then(|base| self.resolve_type(self.qname(extension, base)));
            if let Some(ty) = ty {
                self.validate_text(arena, node, ty, state);
            } else {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "The base type was not found.",
                ));
            }
            attribute_owner = extension;
        } else {
            if self
                .children(rule)
                .any(|child| self.is(child, "complexContent"))
            {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "Complex content derivation is unsupported.",
                ));
                return;
            }
            self.validate_attributes(arena, node, attribute_owner, state);
            if !matches!(self.attr(rule, "mixed"), Some("true" | "1"))
                && arena.children(node).any(|child| {
                    matches!(
                        arena[child].kind,
                        Kind::Text | Kind::Cdata | Kind::EntityReference
                    ) && !arena[child].value.chars().all(xml_space)
                })
            {
                state.diagnostics.push(element_error(arena, node, 1843,
                    "Character content other than whitespace is not allowed because the content type is 'element-only'."));
            }
            let rules: Vec<_> = self
                .children(rule)
                .filter(|&part| self.is_particle(part))
                .collect();
            match self.match_content(arena, node, &rules, state.work) {
                Ok(children) => state.pending.extend(children),
                Err(Mismatch::Unexpected(child)) => state.diagnostics.push(element_error(
                    arena,
                    child,
                    1871,
                    "This element is not expected.",
                )),
                Err(Mismatch::Missing) => state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "Missing child element(s).",
                )),
                Err(Mismatch::Unsupported) => state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "This schema content model is unsupported.",
                )),
                Err(Mismatch::Limit) => state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "Schema validation limit exceeded.",
                )),
            }
            return;
        }
        self.validate_attributes(arena, node, attribute_owner, state);
    }

    fn validate_text(&self, arena: &Arena, node: NodeId, ty: Type, state: &mut Validation<'_>) {
        if elements(arena, node).next().is_some() {
            state.diagnostics.push(element_error(
                arena,
                node,
                1828,
                "Element content is not allowed, because the type definition is simple.",
            ));
            return;
        }
        let mut text = String::new();
        for child in arena.children(node) {
            if matches!(
                arena[child].kind,
                Kind::Text | Kind::Cdata | Kind::EntityReference
            ) {
                text.push_str(&arena[child].value);
            }
        }
        let builtin = match &ty {
            Type::Builtin(name) => Some(name.clone()),
            Type::Declared(_) => None,
        };
        if !self.valid_value(ty, &text) {
            let declared = builtin.map_or_else(
                || "the declared type".to_owned(),
                |name| format!("the atomic type 'xs:{name}'"),
            );
            state.diagnostics.push(element_error(
                arena,
                node,
                1824,
                &format!("'{text}' is not a valid value of {declared}."),
            ));
        }
    }
}
