//! Validate attributes of a complex type and collect optional XSD defaults.
//!
//! ```php
//! $document->schemaValidateSource($schema, LIBXML_SCHEMA_CREATE);
//! ```

use crate::tree::{Arena, Name, NodeId};

use super::{Definition, Rule, Schema, Validation, element_error};
use crate::schema::{XMLNS, XSI, expanded, issue, label};

impl Schema {
    pub(super) fn validate_attributes(
        &self,
        arena: &Arena,
        node: NodeId,
        owner: Rule,
        state: &mut Validation<'_>,
    ) {
        let mut declared = std::collections::HashSet::new();
        let wildcard = self
            .children(owner)
            .find(|&rule| self.is(rule, "anyAttribute"));
        for rule in self.children(owner) {
            if self.is(rule, "attributeGroup") {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "Attribute groups are unsupported.",
                ));
                return;
            }
            if !self.is(rule, "attribute") {
                continue;
            }
            let Some(name) = self.declared_name(rule, Definition::Attribute) else {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "The attribute declaration was not found.",
                ));
                continue;
            };
            let existing = arena[node].attributes().iter().find_map(|&id| {
                let attr = &arena[id];
                (attr.name.local == name.1
                    && attr.name.namespace.as_deref().unwrap_or_default() == name.0)
                    .then_some(attr)
            });
            let use_kind = self.attr(rule, "use");
            let Some(definition) = self.declaration(rule, Definition::Attribute) else {
                continue;
            };
            if let Some(attribute) = existing {
                if use_kind == Some("prohibited") {
                    invalid_attribute(arena, node, &attribute.name, state);
                } else {
                    self.validate_attr_value(
                        arena,
                        node,
                        &attribute.name,
                        &attribute.value,
                        definition,
                        state,
                    );
                }
            } else if use_kind == Some("required") {
                state.diagnostics.push(element_error(
                    arena,
                    node,
                    1868,
                    &format!("The attribute '{}' is required but missing.", name.1),
                ));
            } else if use_kind != Some("prohibited")
                && let Some(defaults) = state.defaults.as_deref_mut()
                && name.0.is_empty()
                && let Some(value) = self
                    .attr(rule, "default")
                    .or_else(|| self.attr(rule, "fixed"))
                    .or_else(|| self.attr(definition, "default"))
                    .or_else(|| self.attr(definition, "fixed"))
            {
                defaults.push((node, Name::new(&name.1), value.into()));
            }
            declared.insert(name);
        }
        for &id in arena[node].attributes() {
            let attr = &arena[id];
            if metadata(&attr.name) {
                continue;
            }
            let name = expanded(&attr.name);
            if declared.contains(&name) {
                continue;
            }
            if let Some(wildcard) = wildcard
                && self.in_namespace(wildcard, &attr.name)
            {
                let process = self.attr(wildcard, "processContents");
                if process == Some("skip") {
                    continue;
                }
                if let Some(rule) = self.named(Definition::Attribute, name) {
                    self.validate_attr_value(arena, node, &attr.name, &attr.value, rule, state);
                    continue;
                }
                if process == Some("lax") {
                    continue;
                }
            }
            invalid_attribute(arena, node, &attr.name, state);
        }
    }

    fn validate_attr_value(
        &self,
        arena: &Arena,
        node: NodeId,
        name: &Name,
        value: &str,
        rule: Rule,
        state: &mut Validation<'_>,
    ) {
        if !self
            .type_of(rule, "string")
            .is_some_and(|ty| self.valid_value(ty, value))
        {
            state.diagnostics.push(element_error(
                arena,
                node,
                1824,
                &format!(
                    "attribute '{}': '{value}' is not a valid value of the declared type.",
                    label(name)
                ),
            ));
        } else if let Some(fixed) = self.attr(rule, "fixed")
            && fixed != value
        {
            state.diagnostics.push(element_error(
                arena,
                node,
                1874,
                &format!(
                    "The value of attribute '{}' does not match its fixed value '{fixed}'.",
                    label(name)
                ),
            ));
        }
    }
}

pub(super) fn metadata(name: &Name) -> bool {
    match name.namespace.as_deref() {
        Some(XMLNS) => true,
        Some(XSI) => matches!(
            name.local.as_str(),
            "schemaLocation" | "noNamespaceSchemaLocation"
        ),
        _ => false,
    }
}

pub(super) fn invalid_attribute(
    arena: &Arena,
    node: NodeId,
    name: &Name,
    state: &mut Validation<'_>,
) {
    let element = label(&arena[node].name);
    let attribute = label(name);
    state.diagnostics.push(issue(1866, arena[node].line, format!(
        "Element '{element}', attribute '{attribute}': The attribute '{attribute}' is not allowed."
    )));
}
