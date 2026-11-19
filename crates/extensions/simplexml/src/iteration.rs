//! Iterate SimpleXML selections using their local node names as keys.
//!
//! ```php
//! foreach ($xml->item as $name => $item) {
//!     echo "$name ", $item['id'];
//! }
//! ```

use std::rc::Rc;

use phera_ext_dom::tree::{Kind, NodeId};
use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use crate::selection::{Cursor, Selection, State, children, data, wrap};

type This<'a> = Option<&'a Rc<Object>>;

pub fn iterator(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    let state = data(&object)?;
    Ok(Box::new(Iter {
        state,
        class: object.class.name().to_owned(),
    }))
}

pub fn refuse_by_reference(
    _: &mut dyn Context,
    _: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Err(NativeError::error(
        "An iterator cannot be used with foreach by reference",
    ))
}

struct Iter {
    state: Rc<State>,
    class: String,
}

impl NativeIterator for Iter {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        *self.state.cursor.borrow_mut() = Cursor {
            nodes: self.state.selection.items(),
            index: 0,
        };
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        let mut cursor = self.state.cursor.borrow_mut();
        cursor.index = cursor.index.saturating_add(1);
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(active(&self.state.cursor.borrow()).is_some())
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        let node = active(&self.state.cursor.borrow()).ok_or_else(inactive)?;
        wrap_current(context, &self.class, &self.state.selection, node)
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        let node = active(&self.state.cursor.borrow()).ok_or_else(inactive)?;
        Ok(key_name(&self.state.selection, node))
    }
}

fn active(cursor: &Cursor) -> Option<NodeId> {
    cursor.nodes.get(cursor.index).copied()
}

fn inactive() -> NativeError {
    NativeError::error("Iterator not initialized or already consumed")
}

fn key_name(selection: &Selection, node: NodeId) -> Value {
    selection
        .anchor
        .document(|doc| Value::from(doc.arena[node].name.local.clone()))
}

fn wrap_current(
    context: &mut dyn Context,
    class: &str,
    selection: &Selection,
    node: NodeId,
) -> NativeResult {
    let node = selection.anchor.select(node).ok_or_else(inactive)?;
    wrap(
        context,
        class,
        Selection::node(node, selection.filter.clone()),
    )
}

fn receiver(this: This<'_>) -> Result<&Object, NativeError> {
    this.map(Rc::as_ref).ok_or_else(inactive)
}

pub fn rewind(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    *state.cursor.borrow_mut() = Cursor {
        nodes: state.selection.items(),
        index: 0,
    };
    Ok(Value::Null)
}

pub fn advance(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let mut cursor = state.cursor.borrow_mut();
    cursor.index = cursor.index.saturating_add(1);
    Ok(Value::Null)
}

pub fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let present = active(&state.cursor.borrow()).is_some();
    Ok(Value::Bool(present))
}

pub fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let node = active(&state.cursor.borrow()).ok_or_else(inactive)?;
    Ok(key_name(&state.selection, node))
}

pub fn current(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    let node = active(&state.cursor.borrow()).ok_or_else(inactive)?;
    wrap_current(context, object.class.name(), &state.selection, node)
}

pub fn has_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let active = active(&state.cursor.borrow());
    Ok(Value::Bool(active.is_some_and(|node| {
        state
            .selection
            .anchor
            .document(|doc| !children(&doc.arena, node, &state.selection.filter, None).is_empty())
    })))
}

pub fn get_children(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    let active = active(&state.cursor.borrow());
    let Some(node) = active.filter(|&node| {
        state
            .selection
            .anchor
            .document(|doc| doc.arena[node].kind == Kind::Element)
    }) else {
        return Ok(Value::Null);
    };
    wrap_current(context, object.class.name(), &state.selection, node)
}
