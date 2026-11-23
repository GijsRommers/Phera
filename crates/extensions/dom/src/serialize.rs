//! Serialize XML or HTML from the arena, using an explicit traversal stack.
//!
//! ```php
//! $document->formatOutput = true;
//! echo $document->saveXML(null, LIBXML_NOEMPTYTAG);
//! ```

mod canonical;
mod escape;
mod html_format;
mod markup;

pub use canonical::{CanonicalOptions, canonical};
use markup::{html_boolean, html_void, push_name, write_doctype};

use std::fmt::Write;

use crate::document::Document;
use crate::tree::{Arena, Kind, Node, NodeId};
use crate::xml::names::XMLNS;

use self::escape::{Escaping, escape};

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub html: bool,
    pub omit_declaration: bool,
    pub expand_empty: bool,
}

#[derive(Clone, Copy)]
enum Mode {
    Xml,
    Html,
}

pub fn save(document: &Document, root: NodeId, options: Options) -> Vec<u8> {
    let whole = root == Arena::DOCUMENT;
    let mut output = String::new();
    let encoding = document.settings.encoding.as_deref().unwrap_or_default();
    if whole && !options.html && !options.omit_declaration {
        let version = document.settings.version.as_deref().unwrap_or("1.0");
        let _ = write!(output, "<?xml version=\"{version}\"");
        if !encoding.is_empty() {
            let _ = write!(output, " encoding=\"{encoding}\"");
        }
        if let Some(standalone) = document.settings.standalone {
            output.push_str(if standalone {
                " standalone=\"yes\""
            } else {
                " standalone=\"no\""
            });
        }
        output.push_str("?>\n");
    }

    let serializer = Serializer {
        arena: &document.arena,
        root,
        mode: if options.html { Mode::Html } else { Mode::Xml },
        pretty: document.settings.format,
        modern: document.settings.modern,
        html_document: document.settings.html,
        expand_empty: options.expand_empty,
        whole,
        ascii_text: whole && (encoding.is_empty() || encoding.eq_ignore_ascii_case("US-ASCII")),
        ascii_attributes: encoding.is_empty()
            || (whole && encoding.eq_ignore_ascii_case("US-ASCII")),
    };
    serializer.write_tree(root, &mut output);
    if whole && !document.settings.modern && !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    if whole && !options.html {
        return escape::encode_xml(output, encoding);
    }
    output.into_bytes()
}

struct Serializer<'a> {
    arena: &'a Arena,
    root: NodeId,
    mode: Mode,
    pretty: bool,
    modern: bool,
    html_document: bool,
    expand_empty: bool,
    whole: bool,
    ascii_text: bool,
    ascii_attributes: bool,
}

enum Action {
    Visit {
        id: NodeId,
        depth: usize,
        pretty: bool,
    },
    Children {
        next: NodeId,
        depth: usize,
        pretty: bool,
        layout: ChildLayout,
    },
    Close {
        id: NodeId,
        depth: usize,
        indent: bool,
    },
    Newline,
}

#[derive(Clone, Copy)]
enum ChildLayout {
    Document,
    Indented,
    Inline,
}

impl Serializer<'_> {
    fn write_tree(&self, root: NodeId, output: &mut String) {
        let mut pending = vec![Action::Visit {
            id: root,
            depth: 0,
            pretty: self.pretty,
        }];
        while let Some(action) = pending.pop() {
            match action {
                Action::Newline => output.push('\n'),
                Action::Close { id, depth, indent } => {
                    if indent {
                        padding(output, depth);
                    }
                    output.push_str("</");
                    push_name(output, &self.arena[id]);
                    output.push('>');
                }
                Action::Visit { id, depth, pretty } => {
                    self.visit(id, depth, pretty, &mut pending, output);
                }
                Action::Children {
                    next,
                    depth,
                    pretty,
                    layout,
                } => {
                    if let Some(sibling) = self.arena[next].next_sibling() {
                        pending.push(Action::Children {
                            next: sibling,
                            depth,
                            pretty,
                            layout,
                        });
                    }
                    if matches!(layout, ChildLayout::Indented)
                        || (matches!(layout, ChildLayout::Document)
                            && self.document_separator(next))
                    {
                        pending.push(Action::Newline);
                    }
                    if matches!(layout, ChildLayout::Indented) {
                        padding(output, depth);
                    }
                    self.visit(next, depth, pretty, &mut pending, output);
                }
            }
        }
    }

    fn visit(
        &self,
        id: NodeId,
        depth: usize,
        pretty: bool,
        pending: &mut Vec<Action>,
        output: &mut String,
    ) {
        let node = &self.arena[id];
        let html = matches!(self.mode, Mode::Html);
        match node.kind {
            Kind::Document | Kind::Fragment => {
                if let Some(next) = node.first_child() {
                    pending.push(Action::Children {
                        next,
                        depth,
                        pretty,
                        layout: if node.kind == Kind::Document {
                            ChildLayout::Document
                        } else {
                            ChildLayout::Inline
                        },
                    });
                }
            }
            Kind::Element => {
                output.push('<');
                push_name(output, node);
                if self.modern && !html && (self.html_document || id == self.root) {
                    self.modern_namespace(id, output);
                }
                self.attributes(id, output);
                let xhtml =
                    self.modern && node.name.namespace.as_deref() == Some(crate::html::XHTML);
                if html && self.void_element(node, xhtml) {
                    output.push('>');
                    return;
                }
                if !html && xhtml && self.void_element(node, true) && node.first_child().is_none() {
                    output.push_str(" />");
                    return;
                }
                if !html && !xhtml && node.first_child().is_none() && !self.expand_empty {
                    output.push_str("/>");
                    return;
                }
                output.push('>');
                if html && pretty {
                    self.formatted_html_children(id, depth, pending, output);
                    return;
                }
                let block = !html
                    && pretty
                    && node.first_child().is_some()
                    && self.arena.children(id).all(|child| {
                        !matches!(
                            self.arena[child].kind,
                            Kind::Text | Kind::Cdata | Kind::EntityReference
                        )
                    });
                pending.push(Action::Close {
                    id,
                    depth,
                    indent: block,
                });
                if let Some(next) = node.first_child() {
                    pending.push(Action::Children {
                        next,
                        depth: depth + 1,
                        pretty: block,
                        layout: if block {
                            ChildLayout::Indented
                        } else {
                            ChildLayout::Inline
                        },
                    });
                }
                if block {
                    output.push('\n');
                }
            }
            Kind::Text | Kind::Cdata => {
                let raw_html = html
                    && node.parent().is_some_and(|parent| {
                        let name = self.arena[parent].name.local.as_str();
                        matches!(name, "script" | "style")
                            || self.modern
                                && matches!(
                                    name,
                                    "xmp"
                                        | "iframe"
                                        | "noembed"
                                        | "noframes"
                                        | "plaintext"
                                        | "noscript"
                                )
                    });
                if node.kind == Kind::Cdata && !html {
                    let _ = write!(output, "<![CDATA[{}]]>", node.value);
                } else if raw_html {
                    output.push_str(&node.value);
                } else {
                    escape(output, &node.value, self.escaping(false));
                }
            }
            Kind::Comment => {
                let _ = write!(output, "<!--{}-->", node.value);
            }
            Kind::ProcessingInstruction => {
                output.push_str("<?");
                push_name(output, node);
                if !node.value.is_empty() {
                    output.push(' ');
                    output.push_str(&node.value);
                }
                output.push_str(if html { ">" } else { "?>" });
            }
            Kind::Doctype if self.modern && html => {
                let _ = write!(output, "<!DOCTYPE {}>", node.name.local);
            }
            Kind::Doctype => write_doctype(node, output),
            Kind::Entity | Kind::Notation | Kind::Namespace => {}
            Kind::EntityReference => {
                let _ = write!(output, "&{};", node.name.local);
            }
            Kind::Attribute => self.attribute(node, output),
        }
    }

    fn document_separator(&self, id: NodeId) -> bool {
        let node = &self.arena[id];
        match self.mode {
            Mode::Xml if self.modern && self.html_document => node.kind == Kind::Doctype,
            Mode::Xml => !self.modern || node.next_sibling().is_some(),
            Mode::Html => !self.modern && node.kind == Kind::Doctype,
        }
    }

    fn attributes(&self, id: NodeId, output: &mut String) {
        let attributes = self.arena[id]
            .attributes()
            .iter()
            .map(|&id| &self.arena[id]);
        let declared = |attribute: &&Node| attribute.name.namespace.as_deref() == Some(XMLNS);
        if self.modern {
            for attribute in attributes {
                self.attribute(attribute, output);
            }
            return;
        }
        for attribute in attributes.clone().filter(declared) {
            self.attribute(attribute, output);
        }
        for attribute in attributes.filter(|attribute| !declared(attribute)) {
            self.attribute(attribute, output);
        }
    }

    fn attribute(&self, attribute: &Node, output: &mut String) {
        output.push(' ');
        push_name(output, attribute);
        if !self.modern && matches!(self.mode, Mode::Html) && html_boolean(&attribute.name.local) {
            return;
        }
        output.push_str("=\"");
        let mode = self.escaping(true);
        if matches!(self.mode, Mode::Xml)
            && let Some(first) = attribute.first_child()
            && (self.arena[first].kind == Kind::EntityReference
                || self.arena[first].next_sibling().is_some())
        {
            for node in std::iter::successors(Some(first), |&id| self.arena[id].next_sibling()) {
                let node = &self.arena[node];
                if node.kind == Kind::EntityReference {
                    let _ = write!(output, "&{};", node.name.local);
                } else {
                    escape(output, &node.value, mode);
                }
            }
        } else {
            escape(output, &attribute.value, mode);
        }
        output.push('"');
    }

    fn escaping(&self, attribute: bool) -> Escaping {
        match self.mode {
            Mode::Xml => Escaping::Xml {
                attribute,
                ascii: if attribute {
                    self.ascii_attributes
                } else {
                    self.ascii_text
                },
            },
            Mode::Html if self.modern => Escaping::Html5 { attribute },
            Mode::Html => Escaping::Html {
                attribute,
                ascii: self.whole,
            },
        }
    }

    fn void_element(&self, node: &Node, xhtml: bool) -> bool {
        if !self.modern {
            return html_void(&node.name.local);
        }
        xhtml && markup::html5_void(&node.name.local)
    }

    fn modern_namespace(&self, id: NodeId, output: &mut String) {
        let node = &self.arena[id];
        let current = node.name.namespace.as_deref();
        let inherited = (id != self.root)
            .then(|| {
                node.parent()
                    .and_then(|parent| self.arena[parent].name.namespace.as_deref())
            })
            .flatten();
        let prefix = node.name.prefix.as_deref();
        let declared = node.attributes().iter().any(|&id| {
            let name = &self.arena[id].name;
            name.namespace.as_deref() == Some(XMLNS)
                && match prefix {
                    Some(prefix) => name.prefix.is_some() && name.local == prefix,
                    None => name.prefix.is_none(),
                }
        });
        if current != inherited && !declared {
            let uri = current.unwrap_or_default();
            if let Some(prefix) = prefix {
                let _ = write!(output, " xmlns:{prefix}=\"");
            } else {
                output.push_str(" xmlns=\"");
            }
            escape(
                output,
                uri,
                Escaping::Xml {
                    attribute: true,
                    ascii: false,
                },
            );
            output.push('"');
        }
    }
}

fn padding(output: &mut String, depth: usize) {
    for _ in 0..depth {
        output.push_str("  ");
    }
}
