//! Traverse a node list or attribute map through PHP's iterator protocol.
//!
//! ```php
//! foreach ($root->childNodes as $child) echo $child->nodeName;
//! ```

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};
use std::rc::Rc;

use super::{Collection, Location, Selection, This, collection, receiver};
use crate::tree::{Kind, NodeId};

pub(super) fn get_iterator(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let cursor = iterator(context, receiver(this)?.clone())?;
    context.create_iterator(cursor)
}

struct Cursor {
    collection: Collection,
    ids: Vec<NodeId>,
    index: usize,
    revision: Option<u64>,
    root: Option<Location>,
    linked_children: bool,
}

impl Cursor {
    fn refresh(&mut self) {
        let root = self.collection.root();
        let state = root.shared.borrow();
        let arena = &state.document.arena;
        let revision = arena.revision();
        let source_changed = self
            .root
            .as_ref()
            .is_none_or(|previous| !root.same_document(previous));
        if source_changed || (!self.linked_children && self.revision != Some(revision)) {
            self.linked_children = matches!(self.collection.selection, Selection::Children)
                && arena[root.id].kind != Kind::EntityReference;
            self.ids = if self.linked_children {
                arena
                    .children(root.id)
                    .find(|&id| arena[id].kind != Kind::Notation)
                    .into_iter()
                    .collect()
            } else {
                self.collection.ids(arena)
            };
            self.revision = Some(revision);
            drop(state);
            self.root = Some(root);
        }
    }
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        if self.index != 0 {
            return Err(NativeError::error("Iterator does not support rewinding"));
        }
        self.root = None;
        self.refresh();
        self.index = 0;
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.refresh();
        if self.linked_children
            && let Some(root) = &self.root
        {
            let state = root.shared.borrow();
            let arena = &state.document.arena;
            if let Some(&current) = self.ids.get(self.index)
                && arena[current].parent() == Some(root.id)
            {
                let siblings = std::iter::successors(arena[current].next_sibling(), |&id| {
                    arena[id].next_sibling()
                });
                self.ids.extend(
                    siblings
                        .filter(|&id| arena[id].kind != Kind::Notation)
                        .take(1),
                );
            }
        }
        self.index = self.index.saturating_add(1);
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        self.refresh();
        Ok(self.index < self.ids.len())
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        self.refresh();
        self.collection
            .wrap_node(context, self.ids.get(self.index).copied())
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        self.refresh();
        if matches!(
            self.collection.selection,
            Selection::Attributes | Selection::Declarations(_)
        ) && let Some(&id) = self.ids.get(self.index)
        {
            let root = self.collection.root();
            let state = root.shared.borrow();
            return Ok(state.document.arena[id].name.local.clone().into());
        }
        Ok(Value::Int(self.index as i64))
    }
}

pub(super) fn iterator(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    let collection = collection(&object)?;
    Ok(Box::new(Cursor {
        collection,
        ids: Vec::new(),
        index: 0,
        revision: None,
        root: None,
        linked_children: false,
    }))
}
