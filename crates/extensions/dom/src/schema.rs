//! Validate an XML document against declarations in a bounded, local XSD set.
//!
//! ```php
//! $document->schemaValidate('services.xsd');
//! ```

mod content;
pub(crate) mod loader;
mod types;

use std::collections::HashMap;

use crate::document::ParseError;
use crate::tree::{Arena, Kind, Name, Node, NodeId};
use crate::xml::names::{XML, XMLNS};

const XSD: &str = "http://www.w3.org/2001/XMLSchema";
const XSI: &str = "http://www.w3.org/2001/XMLSchema-instance";
const MAX_WORK: usize = 1_000_000;
type QName = (String, String);

#[derive(Clone, Copy, Debug)]
struct Rule {
    file: usize,
    node: NodeId,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Definition {
    Element,
    Type,
    Group,
    Attribute,
}

pub(crate) struct SchemaFile {
    arena: Arena,
    root: NodeId,
    namespace: String,
    elements_qualified: bool,
    attributes_qualified: bool,
}

#[derive(Default)]
pub(crate) struct Schema {
    files: Vec<SchemaFile>,
    definitions: HashMap<(Definition, QName), Rule>,
}

impl Schema {
    fn add(&mut self, schema: SchemaFile) {
        let file = self.files.len();
        for node in schema.arena.children(schema.root) {
            let child = &schema.arena[node];
            if child.name.namespace.as_deref() != Some(XSD) {
                continue;
            }
            let definition = match child.name.local.as_str() {
                "element" => Definition::Element,
                "complexType" | "simpleType" => Definition::Type,
                "group" => Definition::Group,
                "attribute" => Definition::Attribute,
                _ => continue,
            };
            if let Some(name) = attr(&schema.arena, node, "name") {
                let key = (definition, (schema.namespace.clone(), name.to_owned()));
                self.definitions.entry(key).or_insert(Rule { file, node });
            }
        }
        self.files.push(schema);
    }

    fn file(&self, rule: Rule) -> &SchemaFile {
        &self.files[rule.file]
    }

    fn node(&self, rule: Rule) -> &Node {
        &self.file(rule).arena[rule.node]
    }

    fn attr(&self, rule: Rule, name: &str) -> Option<&str> {
        attr(&self.file(rule).arena, rule.node, name)
    }

    fn children(&self, rule: Rule) -> impl Iterator<Item = Rule> + '_ {
        let arena = &self.file(rule).arena;
        arena.children(rule.node).filter_map(move |node| {
            (arena[node].kind == Kind::Element
                && arena[node].name.namespace.as_deref() == Some(XSD))
            .then_some(Rule { node, ..rule })
        })
    }

    fn is(&self, rule: Rule, name: &str) -> bool {
        self.node(rule).name.local == name
    }

    fn qname(&self, rule: Rule, value: &str) -> QName {
        let (prefix, local) = value.split_once(':').unwrap_or(("", value));
        let arena = &self.file(rule).arena;
        let namespace = if prefix == "xml" {
            Some(XML)
        } else {
            std::iter::once(rule.node)
                .chain(arena.ancestors(rule.node))
                .find_map(|node| {
                    arena[node].attributes().iter().find_map(|&id| {
                        let attr = &arena[id];
                        (attr.name.namespace.as_deref() == Some(XMLNS)
                            && if prefix.is_empty() {
                                attr.name.prefix.is_none() && attr.name.local == "xmlns"
                            } else {
                                attr.name.prefix.as_deref() == Some("xmlns")
                                    && attr.name.local == prefix
                            })
                        .then_some(attr.value.as_str())
                    })
                })
        };
        let namespace = namespace.unwrap_or(prefix);
        (namespace.into(), local.into())
    }

    fn named(&self, kind: Definition, name: QName) -> Option<Rule> {
        self.definitions.get(&(kind, name)).copied()
    }

    fn declaration(&self, rule: Rule, kind: Definition) -> Option<Rule> {
        if let Some(reference) = self.attr(rule, "ref") {
            self.named(kind, self.qname(rule, reference))
        } else {
            Some(rule)
        }
    }

    fn declared_name(&self, rule: Rule, kind: Definition) -> Option<QName> {
        let rule = self.declaration(rule, kind)?;
        let file = self.file(rule);
        let qualified = self.node(rule).parent() == Some(file.root)
            || match self.attr(rule, "form") {
                Some(form) => form == "qualified",
                None if kind == Definition::Attribute => file.attributes_qualified,
                None => file.elements_qualified,
            };
        let namespace = if qualified {
            file.namespace.clone()
        } else {
            String::new()
        };
        Some((namespace, self.attr(rule, "name")?.into()))
    }

    fn in_namespace(&self, rule: Rule, name: &Name) -> bool {
        let namespace = name.namespace.as_deref().unwrap_or_default();
        let target = &self.file(rule).namespace;
        self.attr(rule, "namespace")
            .unwrap_or("##any")
            .split_ascii_whitespace()
            .any(|item| match item {
                "##any" => true,
                "##other" => !namespace.is_empty() && namespace != target,
                "##targetNamespace" => namespace == target,
                "##local" => namespace.is_empty(),
                value => value == namespace,
            })
    }

    pub(crate) fn validate(&self, arena: &mut Arena, defaults: bool) -> Vec<ParseError> {
        let Some(root) = elements(arena, Arena::DOCUMENT).next() else {
            return vec![issue(
                1872,
                0,
                "The document has no document element.".into(),
            )];
        };
        let Some(declaration) = self.named(Definition::Element, expanded(&arena[root].name)) else {
            return vec![element_error(
                arena,
                root,
                1845,
                "No matching global declaration available for the validation root.",
            )];
        };
        let mut pending = vec![(root, declaration)];
        let mut work = MAX_WORK;
        let mut diagnostics = Vec::new();
        let mut insert = Vec::new();
        while let Some((node, element)) = pending.pop() {
            if work == 0 || diagnostics.len() >= 100 {
                diagnostics.push(element_error(
                    arena,
                    node,
                    1871,
                    "Schema validation limit exceeded.",
                ));
                break;
            }
            work -= 1;
            self.validate_element(
                arena,
                node,
                element,
                &mut Validation {
                    work: &mut work,
                    pending: &mut pending,
                    diagnostics: &mut diagnostics,
                    defaults: defaults.then_some(&mut insert),
                },
            );
        }
        for (node, name, value) in insert {
            let attribute = arena.allocate(Node::new(Kind::Attribute, name, value));
            let _ = arena.set_attribute(node, attribute, None);
        }
        diagnostics
    }
}

struct Validation<'a> {
    work: &'a mut usize,
    pending: &'a mut Vec<(NodeId, Rule)>,
    diagnostics: &'a mut Vec<ParseError>,
    defaults: Option<&'a mut Vec<(NodeId, Name, String)>>,
}

fn attr<'a>(arena: &'a Arena, node: NodeId, name: &str) -> Option<&'a str> {
    arena[node].attributes().iter().find_map(|&id| {
        let attr = &arena[id];
        (attr.name.namespace.is_none() && attr.name.local == name).then_some(attr.value.as_str())
    })
}

fn elements(arena: &Arena, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    arena
        .children(node)
        .filter(|&node| arena[node].kind == Kind::Element)
}

fn expanded(name: &Name) -> QName {
    (
        name.namespace.clone().unwrap_or_default(),
        name.local.clone(),
    )
}

fn label(name: &Name) -> String {
    name.namespace
        .as_ref()
        .filter(|ns| !ns.is_empty())
        .map_or_else(
            || name.local.clone(),
            |ns| format!("{{{ns}}}{}", name.local),
        )
}

fn element_error(arena: &Arena, node: NodeId, code: i64, message: &str) -> ParseError {
    issue(
        code,
        arena[node].line,
        format!("Element '{}': {message}", label(&arena[node].name)),
    )
}

fn issue(code: i64, line: usize, message: String) -> ParseError {
    ParseError {
        code,
        line,
        column: 0,
        level: 2,
        message: format!("{message}\n"),
        file: String::new(),
    }
}
