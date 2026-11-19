//! Traverse a node list or attribute map through PHP's iterator protocol.
//!
//! ```php
//! foreach ($root->childNodes as $child) echo $child->nodeName;
//! ```

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};
use std::rc::Rc;

use super::{Collection, Selection, This, collection, receiver};
use crate::tree::NodeId;

pub(super) fn get_iterator(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let cursor = iterator(context, receiver(this)?.clone())?;
    context.create_iterator(cursor)
}

struct Cursor {
    collection: Collection,
    ids: Vec<NodeId>,
    index: usize,
    revision: Option<u64>,
}

impl Cursor {
    fn refresh(&mut self) {
        let state = self.collection.root.shared.borrow();
        let arena = &state.document.arena;
        let revision = arena.revision();
        if self.revision != Some(revision) {
            self.ids = self.collection.ids(arena);
            self.revision = Some(revision);
        }
    }
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.refresh();
        self.index = 0;
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
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
        if matches!(self.collection.selection, Selection::Attributes)
            && let Some(&id) = self.ids.get(self.index)
        {
            let state = self.collection.root.shared.borrow();
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
    }))
}
