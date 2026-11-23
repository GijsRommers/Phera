//! Write canonical XML with namespace scopes and optional XPath selection.
//!
//! ```php
//! $document->loadXML('<r z="1" a="2"/>');
//! echo $document->C14N(); // <r a="2" z="1"></r>
//! ```

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

use crate::document::Document;
use crate::tree::{Arena, Kind, Node, NodeId};
use crate::xml::names::{XML, XMLNS};

use super::escape::{Escaping, escape};
use super::markup::push_name;

#[derive(Default)]
pub struct CanonicalOptions<'a> {
    pub exclusive: bool,
    pub with_comments: bool,
    pub selection: Option<&'a HashSet<NodeId>>,
    pub prefixes: &'a [String],
}

type Namespaces<'a> = BTreeMap<&'a str, (&'a str, Option<NodeId>)>;
type Changes<'a> = Vec<(&'a str, Option<(&'a str, Option<NodeId>)>)>;

enum Event<'a> {
    Open(NodeId),
    Close {
        id: NodeId,
        visible: bool,
        scope: Changes<'a>,
        rendered: Changes<'a>,
    },
}

pub fn canonical(document: &Document, root: NodeId, options: CanonicalOptions<'_>) -> String {
    let arena = &document.arena;
    let mut scope = Namespaces::new();
    let mut rendered = Namespaces::new();
    let mut output = String::new();
    let ancestors: Vec<_> = arena.ancestors(root).collect();
    for ancestor in ancestors.into_iter().rev() {
        add_scope(arena, ancestor, &mut scope);
    }
    let mut events = vec![Event::Open(root)];
    let mut after_document_element = false;
    let visible = |id| options.selection.is_none_or(|nodes| nodes.contains(&id));

    while let Some(event) = events.pop() {
        match event {
            Event::Close {
                id,
                visible,
                scope: changes,
                rendered: written,
            } => {
                if visible {
                    output.push_str("</");
                    push_name(&mut output, &arena[id]);
                    output.push('>');
                    if arena[id].parent() == Some(Arena::DOCUMENT) {
                        after_document_element = true;
                    }
                }
                restore(&mut scope, changes);
                restore(&mut rendered, written);
            }
            Event::Open(id) => {
                let node = &arena[id];
                match node.kind {
                    Kind::Element => {
                        let changes = add_scope(arena, id, &mut scope);
                        let show = visible(id);
                        let written = if show {
                            output.push('<');
                            push_name(&mut output, node);
                            let changes = write_namespaces(
                                &mut output,
                                arena,
                                id,
                                &scope,
                                &mut rendered,
                                &options,
                            );
                            write_attributes(&mut output, arena, root, id, &options);
                            output.push('>');
                            changes
                        } else {
                            Vec::new()
                        };
                        events.push(Event::Close {
                            id,
                            visible: show,
                            scope: changes,
                            rendered: written,
                        });
                        schedule_children(arena, id, &mut events);
                    }
                    Kind::Document | Kind::Fragment => schedule_children(arena, id, &mut events),
                    Kind::Text | Kind::Cdata | Kind::EntityReference if visible(id) => {
                        escape(
                            &mut output,
                            &node.value,
                            Escaping::Canonical { attribute: false },
                        );
                    }
                    Kind::ProcessingInstruction | Kind::Comment
                        if visible(id) && (node.kind != Kind::Comment || options.with_comments) =>
                    {
                        let exterior = root == Arena::DOCUMENT && node.parent() == Some(root);
                        if exterior && after_document_element {
                            output.push('\n');
                        }
                        write_misc(&mut output, node);
                        if exterior && !after_document_element {
                            output.push('\n');
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    output
}

fn schedule_children<'a>(arena: &Arena, id: NodeId, events: &mut Vec<Event<'a>>) {
    let mut child = arena[id].last_child();
    while let Some(id) = child {
        events.push(Event::Open(id));
        child = arena[id].previous_sibling();
    }
}

fn add_scope<'a>(arena: &'a Arena, id: NodeId, scope: &mut Namespaces<'a>) -> Changes<'a> {
    let mut changes = Vec::new();
    let node = &arena[id];
    for &attribute in node.attributes() {
        let attr = &arena[attribute];
        if attr.name.namespace.as_deref() == Some(XMLNS) {
            let prefix = if attr.name.prefix.is_some() {
                attr.name.local.as_str()
            } else {
                ""
            };
            changes.push((prefix, scope.insert(prefix, (&attr.value, Some(attribute)))));
        } else if let (Some(prefix), Some(uri)) = (&attr.name.prefix, &attr.name.namespace)
            && !scope
                .get(prefix.as_str())
                .is_some_and(|(bound, _)| *bound == uri)
        {
            changes.push((prefix, scope.insert(prefix, (uri, None))));
        }
    }
    if node.kind == Kind::Element
        && let Some(uri) = &node.name.namespace
    {
        let prefix = node.name.prefix.as_deref().unwrap_or("");
        if !scope.get(prefix).is_some_and(|(bound, _)| *bound == uri) {
            changes.push((prefix, scope.insert(prefix, (uri, None))));
        }
    }
    changes
}

fn restore<'a>(scope: &mut Namespaces<'a>, changes: Changes<'a>) {
    for (prefix, previous) in changes.into_iter().rev() {
        if let Some(previous) = previous {
            scope.insert(prefix, previous);
        } else {
            scope.remove(prefix);
        }
    }
}

fn write_namespaces<'a>(
    output: &mut String,
    arena: &'a Arena,
    id: NodeId,
    scope: &Namespaces<'a>,
    rendered: &mut Namespaces<'a>,
    options: &CanonicalOptions<'_>,
) -> Changes<'a> {
    let element = &arena[id];
    let mut used = BTreeSet::new();
    if options.exclusive {
        used.insert(element.name.prefix.as_deref().unwrap_or(""));
        for &attr in element.attributes() {
            if options.selection.is_none_or(|nodes| nodes.contains(&attr))
                && let Some(prefix) = arena[attr].name.prefix.as_deref()
            {
                used.insert(prefix);
            }
        }
        used.extend(options.prefixes.iter().map(|prefix| {
            if prefix == "#default" {
                ""
            } else {
                prefix.as_str()
            }
        }));
    }
    let mut changes = Vec::new();
    for (&prefix, &(uri, declaration)) in scope {
        if prefix == "xml"
            || (options.exclusive && !used.contains(prefix))
            || options.selection.is_some_and(|nodes| {
                declaration.is_some_and(|declaration| !nodes.contains(&declaration))
                    && !arena.xpath_namespaces(id).iter().any(|&namespace| {
                        arena[namespace].name.local == prefix && nodes.contains(&namespace)
                    })
            })
            || rendered.get(prefix).map_or("", |(uri, _)| uri) == uri
        {
            continue;
        }
        output.push_str(" xmlns");
        if !prefix.is_empty() {
            output.push(':');
            output.push_str(prefix);
        }
        output.push_str("=\"");
        escape(output, uri, Escaping::Canonical { attribute: true });
        output.push('"');
        changes.push((prefix, rendered.insert(prefix, (uri, declaration))));
    }
    changes
}

fn write_attributes(
    output: &mut String,
    arena: &Arena,
    root: NodeId,
    id: NodeId,
    opts: &CanonicalOptions<'_>,
) {
    let mut attributes = BTreeMap::new();
    for &attr in arena[id].attributes() {
        let node = &arena[attr];
        if node.name.namespace.as_deref() != Some(XMLNS)
            && opts
                .selection
                .is_none_or(|selected| selected.contains(&attr))
        {
            attributes.insert(
                (node.name.namespace.as_deref(), node.name.local.as_str()),
                node,
            );
        }
    }
    if !opts.exclusive && id == root && opts.selection.is_none() {
        for ancestor in arena.ancestors(root) {
            for &attribute in arena[ancestor].attributes() {
                let node = &arena[attribute];
                if node.name.namespace.as_deref() == Some(XML) {
                    attributes
                        .entry((Some(XML), node.name.local.as_str()))
                        .or_insert(node);
                }
            }
        }
    }
    for attribute in attributes.values() {
        output.push(' ');
        push_name(output, attribute);
        output.push_str("=\"");
        escape(
            output,
            &attribute.value,
            Escaping::Canonical { attribute: true },
        );
        output.push('"');
    }
}

fn write_misc(output: &mut String, node: &Node) {
    if node.kind == Kind::Comment {
        let _ = write!(output, "<!--{}-->", node.value.replace('\r', "&#xD;"));
    } else {
        output.push_str("<?");
        push_name(output, node);
        if !node.value.is_empty() {
            output.push(' ');
            output.push_str(&node.value.replace('\r', "&#xD;"));
        }
        output.push_str("?>");
    }
}
