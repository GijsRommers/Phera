//! Iterate SimpleXML selections using their local node names as keys.
//!
//! ```php
//! foreach ($xml->item as $name => $item) {
//!     echo "$name ", $item['id'];
//! }
//! ```

use std::rc::Rc;

use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::Kind;
use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use crate::selection::{Selection, State, children, data, wrap};

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
        rewind_cursor(&self.state);
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        advance_cursor(&self.state);
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(self.state.cursor.borrow().is_some())
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        let node = self.state.cursor.borrow().clone().ok_or_else(inactive)?;
        wrap_current(context, &self.class, &self.state.selection, node)
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        let cursor = self.state.cursor.borrow();
        Ok(key_name(cursor.as_ref().ok_or_else(inactive)?))
    }
}

fn rewind_cursor(state: &State) {
    *state.cursor.borrow_mut() = state.selection.next_item(None);
}

fn advance_cursor(state: &State) {
    let mut cursor = state.cursor.borrow_mut();
    *cursor = cursor
        .as_ref()
        .and_then(|node| state.selection.next_item(Some(node)));
}

fn inactive() -> NativeError {
    NativeError::error("Iterator not initialized or already consumed")
}

fn key_name(node: &SharedNode) -> Value {
    node.document(|doc| Value::from(doc.arena[node.id()].name.local.clone()))
}

fn wrap_current(
    context: &mut dyn Context,
    class: &str,
    selection: &Selection,
    node: SharedNode,
) -> NativeResult {
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
    rewind_cursor(&state);
    Ok(Value::Null)
}

pub fn advance(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    advance_cursor(&state);
    Ok(Value::Null)
}

pub fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let present = state.cursor.borrow().is_some();
    Ok(Value::Bool(present))
}

pub fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let cursor = state.cursor.borrow();
    Ok(key_name(cursor.as_ref().ok_or_else(inactive)?))
}

pub fn current(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    let node = state.cursor.borrow().clone().ok_or_else(inactive)?;
    wrap_current(context, object.class.name(), &state.selection, node)
}

pub fn has_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let cursor = state.cursor.borrow();
    Ok(Value::Bool(cursor.as_ref().is_some_and(|node| {
        node.document(|doc| {
            !children(&doc.arena, node.id(), &state.selection.filter, None).is_empty()
        })
    })))
}

pub fn get_children(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    let active = state.cursor.borrow().clone();
    let Some(node) =
        active.filter(|node| node.document(|doc| doc.arena[node.id()].kind == Kind::Element))
    else {
        return Ok(Value::Null);
    };
    wrap_current(context, object.class.name(), &state.selection, node)
}
