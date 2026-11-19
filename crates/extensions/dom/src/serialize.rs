//! Serialize XML or HTML from the arena, using an explicit traversal stack.
//!
//! ```php
//! $document->formatOutput = true;
//! echo $document->saveXML(null, LIBXML_NOEMPTYTAG);
//! ```

mod canonical;
mod escape;

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
    Canonical {
        with_comments: bool,
        exclusive: bool,
    },
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
        mode: if options.html { Mode::Html } else { Mode::Xml },
        pretty: document.settings.format && !options.html,
        expand_empty: options.expand_empty,
        whole,
        ascii_text: whole && (encoding.is_empty() || encoding.eq_ignore_ascii_case("US-ASCII")),
        ascii_attributes: encoding.is_empty()
            || (whole && encoding.eq_ignore_ascii_case("US-ASCII")),
    };
    serializer.write_tree(root, &mut output);
    if whole && !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    if whole && !options.html {
        return escape::encode_xml(output, encoding);
    }
    output.into_bytes()
}

pub fn canonical(
    document: &Document,
    root: NodeId,
    exclusive: bool,
    with_comments: bool,
) -> String {
    let serializer = Serializer {
        arena: &document.arena,
        mode: Mode::Canonical {
            with_comments,
            exclusive,
        },
        pretty: false,
        expand_empty: true,
        whole: root == Arena::DOCUMENT,
        ascii_text: false,
        ascii_attributes: false,
    };
    let mut output = String::new();
    serializer.write_tree(root, &mut output);
    output
}

struct Serializer<'a> {
    arena: &'a Arena,
    mode: Mode,
    pretty: bool,
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
        indent: bool,
    },
    Close {
        id: NodeId,
        depth: usize,
        indent: bool,
    },
    Newline,
}

impl Serializer<'_> {
    fn write_tree(&self, root: NodeId, output: &mut String) {
        let mut pending = vec![Action::Visit {
            id: root,
            depth: 0,
            pretty: self.pretty,
            indent: false,
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
                Action::Visit {
                    id,
                    depth,
                    pretty,
                    indent,
                } => {
                    if indent {
                        padding(output, depth);
                    }
                    self.visit(root, id, depth, pretty, &mut pending, output);
                }
            }
        }
    }

    fn visit(
        &self,
        root: NodeId,
        id: NodeId,
        depth: usize,
        pretty: bool,
        pending: &mut Vec<Action>,
        output: &mut String,
    ) {
        let node = &self.arena[id];
        let canonical = matches!(self.mode, Mode::Canonical { .. });
        let html = matches!(self.mode, Mode::Html);
        match node.kind {
            Kind::Document | Kind::Fragment => {
                let mut child = node.last_child();
                while let Some(id) = child {
                    child = self.arena[id].previous_sibling();
                    if node.kind == Kind::Document && self.document_separator(id) {
                        pending.push(Action::Newline);
                    }
                    pending.push(Action::Visit {
                        id,
                        depth,
                        pretty,
                        indent: false,
                    });
                }
            }
            Kind::Element => {
                output.push('<');
                push_name(output, node);
                if let Mode::Canonical { exclusive, .. } = self.mode {
                    self.canonical_attributes(root, id, exclusive, output);
                } else {
                    self.attributes(id, output);
                }
                if html && html_void(&node.name.local) {
                    output.push('>');
                    return;
                }
                if !html && node.first_child().is_none() && !self.expand_empty {
                    output.push_str("/>");
                    return;
                }
                output.push('>');
                let block = pretty
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
                let mut child = node.last_child();
                while let Some(id) = child {
                    child = self.arena[id].previous_sibling();
                    if block {
                        pending.push(Action::Newline);
                    }
                    pending.push(Action::Visit {
                        id,
                        depth: depth + 1,
                        indent: block,
                        pretty: block,
                    });
                }
                if block {
                    output.push('\n');
                }
            }
            Kind::Text | Kind::Cdata => {
                let raw_html = html
                    && node.parent().is_some_and(|parent| {
                        matches!(self.arena[parent].name.local.as_str(), "script" | "style")
                    });
                if node.kind == Kind::Cdata && !canonical && !html {
                    let _ = write!(output, "<![CDATA[{}]]>", node.value);
                } else if raw_html {
                    output.push_str(&node.value);
                } else {
                    escape(output, &node.value, self.escaping(false));
                }
            }
            Kind::Comment => {
                if !matches!(
                    self.mode,
                    Mode::Canonical {
                        with_comments: false,
                        ..
                    }
                ) {
                    self.canonical_before(id, output);
                    let _ = write!(output, "<!--{}-->", node.value);
                }
            }
            Kind::ProcessingInstruction => {
                self.canonical_before(id, output);
                output.push_str("<?");
                push_name(output, node);
                if !node.value.is_empty() {
                    output.push(' ');
                    output.push_str(&node.value);
                }
                output.push_str(if html { ">" } else { "?>" });
            }
            Kind::Doctype if !canonical => write_doctype(node, output),
            Kind::EntityReference if canonical => escape(output, &node.value, self.escaping(false)),
            Kind::EntityReference => {
                let _ = write!(output, "&{};", node.name.local);
            }
            Kind::Attribute => self.attribute(node, output),
            Kind::Doctype => {}
        }
    }

    fn document_separator(&self, id: NodeId) -> bool {
        let node = &self.arena[id];
        match self.mode {
            Mode::Xml => true,
            Mode::Html => node.kind == Kind::Doctype,
            Mode::Canonical { with_comments, .. } => {
                matches!(node.kind, Kind::ProcessingInstruction)
                    || (with_comments && node.kind == Kind::Comment)
            }
        }
    }

    fn attributes(&self, id: NodeId, output: &mut String) {
        let node = &self.arena[id];
        let (namespaces, attributes): (Vec<_>, Vec<_>) = node
            .attributes()
            .iter()
            .map(|&id| &self.arena[id])
            .partition(|attr| attr.name.namespace.as_deref() == Some(XMLNS));
        for attribute in namespaces.into_iter().chain(attributes) {
            self.attribute(attribute, output);
        }
    }

    fn attribute(&self, attribute: &Node, output: &mut String) {
        output.push(' ');
        push_name(output, attribute);
        if matches!(self.mode, Mode::Html) && html_boolean(&attribute.name.local) {
            return;
        }
        output.push_str("=\"");
        escape(output, &attribute.value, self.escaping(true));
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
            Mode::Html => Escaping::Html {
                attribute,
                ascii: self.whole,
            },
            Mode::Canonical { .. } => Escaping::Canonical { attribute },
        }
    }
}

fn push_name(output: &mut String, node: &Node) {
    if let Some(prefix) = &node.name.prefix {
        output.push_str(prefix);
        output.push(':');
    }
    output.push_str(&node.name.local);
}

fn padding(output: &mut String, depth: usize) {
    for _ in 0..depth {
        output.push_str("  ");
    }
}

fn write_doctype(node: &Node, output: &mut String) {
    let _ = write!(output, "<!DOCTYPE {}", node.name.local);
    if let Some(doctype) = &node.doctype {
        if !doctype.public_id.is_empty() {
            let _ = write!(output, " PUBLIC \"{}\"", doctype.public_id);
        }
        if !doctype.system_id.is_empty() {
            if doctype.public_id.is_empty() {
                output.push_str(" SYSTEM");
            }
            let _ = write!(output, " \"{}\"", doctype.system_id);
        }
        if let Some(subset) = &doctype.internal_subset {
            let _ = write!(output, " [\n{subset}]");
        }
    }
    output.push('>');
}

fn html_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "basefont"
            | "br"
            | "col"
            | "embed"
            | "frame"
            | "hr"
            | "img"
            | "input"
            | "isindex"
            | "keygen"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn html_boolean(name: &str) -> bool {
    matches!(
        name,
        "checked"
            | "compact"
            | "declare"
            | "defer"
            | "disabled"
            | "ismap"
            | "multiple"
            | "nohref"
            | "noresize"
            | "noshade"
            | "nowrap"
            | "readonly"
            | "selected"
    )
}
