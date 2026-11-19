//! Build html5ever's tree directly in the document arena.
//!
//! ```php
//! $document->loadHTML('<p>one<b>two</b>three');
//! ```

use std::borrow::Cow;
use std::cell::{Cell, RefCell};

use html5ever::interface::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::StrTendril;
use html5ever::{Attribute, Namespace, QualName};

use crate::tree::{Arena, Doctype, Kind, Name, Node, NodeId};

pub(crate) const XHTML: &str = "http://www.w3.org/1999/xhtml";

#[derive(Default)]
pub(super) struct Sink {
    arena: RefCell<Arena>,
    line: Cell<usize>,
    pub(super) modern: bool,
}

#[derive(Clone)]
pub(super) struct Handle {
    id: NodeId,
    name: QualName,
    template: Option<NodeId>,
    integration_point: bool,
}

impl Handle {
    fn other(id: NodeId) -> Self {
        Self {
            id,
            name: QualName::new(None, Namespace::default(), "".into()),
            template: None,
            integration_point: false,
        }
    }
}

impl Sink {
    fn insert(&self, parent: NodeId, child: NodeOrText<Handle>, before: Option<NodeId>) {
        let mut arena = self.arena.borrow_mut();
        let child = match child {
            NodeOrText::AppendNode(child) => child.id,
            NodeOrText::AppendText(text) => {
                let previous = match before {
                    Some(sibling) => arena[sibling].previous_sibling(),
                    None => arena[parent].last_child(),
                };
                if let Some(previous) = previous
                    && arena[previous].kind == Kind::Text
                {
                    arena[previous].value.push_str(&text);
                    return;
                }
                arena.allocate(Node::new(Kind::Text, Name::new("#text"), text.to_string()))
            }
        };
        let _ = arena.insert(parent, child, before);
    }

    fn allocate(&self, kind: Kind, name: impl Into<String>, value: impl Into<String>) -> Handle {
        let mut node = Node::new(kind, Name::new(name), value);
        node.line = self.line.get();
        Handle::other(self.arena.borrow_mut().allocate(node))
    }
}

impl TreeSink for Sink {
    type Handle = Handle;
    type Output = Arena;
    type ElemName<'a> = &'a QualName;

    fn finish(self) -> Arena {
        self.arena.into_inner()
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {
        // HTML5 tokenization diagnostics differ from libxml's legacy HTML diagnostics.
    }

    fn get_document(&self) -> Handle {
        Handle::other(Arena::DOCUMENT)
    }

    fn elem_name<'a>(&'a self, target: &'a Handle) -> Self::ElemName<'a> {
        &target.name
    }

    fn create_element(&self, name: QualName, attrs: Vec<Attribute>, flags: ElementFlags) -> Handle {
        let mut arena = self.arena.borrow_mut();
        let mut node = Node::new(Kind::Element, arena_name(&name, self.modern), "");
        node.line = self.line.get();
        let id = arena.allocate(node);
        let template = flags.template.then(|| {
            arena.allocate(Node::new(
                Kind::Fragment,
                Name::new("#document-fragment"),
                "",
            ))
        });
        for attribute in attrs {
            add_attribute(&mut arena, id, attribute);
        }
        Handle {
            id,
            name,
            template,
            integration_point: flags.mathml_annotation_xml_integration_point,
        }
    }

    fn create_comment(&self, text: StrTendril) -> Handle {
        self.allocate(Kind::Comment, "#comment", text.to_string())
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Handle {
        self.allocate(
            Kind::ProcessingInstruction,
            target.to_string(),
            data.to_string(),
        )
    }

    fn append(&self, parent: &Handle, child: NodeOrText<Handle>) {
        self.insert(parent.id, child, None);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Handle,
        previous: &Handle,
        child: NodeOrText<Handle>,
    ) {
        if self.arena.borrow()[element.id].parent().is_some() {
            self.append_before_sibling(element, child);
        } else {
            self.append(previous, child);
        }
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        let handle = self.allocate(Kind::Doctype, name.to_string(), "");
        let mut arena = self.arena.borrow_mut();
        arena[handle.id].doctype = Some(Box::new(Doctype {
            public_id: public_id.to_string(),
            system_id: system_id.to_string(),
            ..Doctype::default()
        }));
        let _ = arena.insert(Arena::DOCUMENT, handle.id, None);
    }

    fn get_template_contents(&self, target: &Handle) -> Handle {
        Handle::other(target.template.unwrap_or(target.id))
    }

    fn same_node(&self, x: &Handle, y: &Handle) -> bool {
        x.id == y.id
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {}

    fn append_before_sibling(&self, sibling: &Handle, child: NodeOrText<Handle>) {
        let parent = self.arena.borrow()[sibling.id].parent();
        if let Some(parent) = parent {
            self.insert(parent, child, Some(sibling.id));
        }
    }

    fn add_attrs_if_missing(&self, target: &Handle, attrs: Vec<Attribute>) {
        let mut arena = self.arena.borrow_mut();
        for attribute in attrs {
            let name = arena_name(&attribute.name, self.modern);
            if !arena[target.id]
                .attributes()
                .iter()
                .any(|&id| arena[id].name == name)
            {
                add_attribute(&mut arena, target.id, attribute);
            }
        }
    }

    fn remove_from_parent(&self, target: &Handle) {
        self.arena.borrow_mut().detach(target.id);
    }

    fn reparent_children(&self, node: &Handle, new_parent: &Handle) {
        let mut arena = self.arena.borrow_mut();
        let children: Vec<_> = arena.children(node.id).collect();
        for child in children {
            let _ = arena.insert(new_parent.id, child, None);
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Handle) -> bool {
        handle.integration_point
    }

    fn set_current_line(&self, line: u64) {
        self.line.set(line as usize);
    }
}

fn arena_name(name: &QualName, modern: bool) -> Name {
    Name {
        local: name.local.to_string(),
        prefix: name.prefix.as_ref().map(ToString::to_string),
        namespace: (!name.ns.is_empty() && (modern || &*name.ns != XHTML))
            .then(|| name.ns.to_string()),
    }
}

fn add_attribute(arena: &mut Arena, element: NodeId, attribute: Attribute) {
    let mut node = Node::new(
        Kind::Attribute,
        arena_name(&attribute.name, true),
        attribute.value.to_string(),
    );
    node.is_id = node.name.namespace.is_none() && node.name.local == "id";
    let id = arena.attribute(node, true);
    let _ = arena.set_attribute(element, id, None);
}
