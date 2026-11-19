//! Character-data operations count Unicode characters rather than UTF-8 bytes.
//!
//! ```php
//! $text = new DOMText('aéz');
//! echo $text->substringData(1, 1); // é
//! ```

mod registration;

pub(super) use registration::register;

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::tree::{Arena, Kind, Name, Node};
use crate::xml;

use super::{This, dom_error, factory, libxml, mutation, this_node};

#[derive(Clone, Copy)]
enum Operation {
    Append,
    Substring,
    Insert,
    Delete,
    Replace,
}

fn change(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    operation: Operation,
) -> NativeResult {
    let node = this_node(this)?;
    if matches!(operation, Operation::Append) {
        let mut state = node.shared.borrow_mut();
        let arena = &mut state.document.arena;
        arena[node.id]
            .value
            .push_str(&factory::argument(arguments, 0).unwrap_or_default());
        arena.refresh_parent_attribute(node.id);
        return Ok(Value::Bool(true));
    }
    let offset = args::int(arguments, 0).unwrap_or_default();
    let count = if matches!(operation, Operation::Insert) {
        0
    } else {
        args::int(arguments, 1).unwrap_or_default()
    };
    let range = {
        let state = node.shared.borrow();
        bounds(&state.document.arena[node.id].value, offset, count)
    };
    let Some(range) = range else {
        return mutation::fail(context, &node, dom_error(1, "Index Size Error"));
    };
    let mut state = node.shared.borrow_mut();
    let value = &mut state.document.arena[node.id].value;
    match operation {
        Operation::Substring => return Ok(Value::from(&value[range])),
        Operation::Delete => value.replace_range(range, ""),
        Operation::Insert | Operation::Replace => {
            let index = if matches!(operation, Operation::Insert) {
                1
            } else {
                2
            };
            value.replace_range(
                range,
                &factory::argument(arguments, index).unwrap_or_default(),
            );
        }
        Operation::Append => {}
    }
    state.document.arena.refresh_parent_attribute(node.id);
    Ok(Value::Bool(true))
}

fn bounds(value: &str, offset: i64, count: i64) -> Option<std::ops::Range<usize>> {
    let offset = usize::try_from(offset).ok()?;
    let count = usize::try_from(count).ok()?;
    let mut positions = value
        .char_indices()
        .map(|(index, _)| index)
        .chain([value.len()]);
    let start = positions.nth(offset)?;
    let end = if count == 0 {
        start
    } else {
        positions.nth(count - 1).unwrap_or(value.len())
    };
    Some(start..end)
}

fn split(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let offset = args::int(arguments, 0).unwrap_or_default();
    if offset < 0 {
        return Err(NativeError::value_error(
            "DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0",
        ));
    }
    let mut state = node.shared.borrow_mut();
    let arena = &mut state.document.arena;
    let Some(range) = bounds(&arena[node.id].value, offset, 0) else {
        return Ok(Value::Bool(false));
    };
    let right = arena[node.id].value.split_off(range.start);
    let id = arena.allocate(Node::new(Kind::Text, Name::new("#text"), right));
    if let Some(parent) = arena[node.id].parent() {
        let next = arena[node.id].next_sibling();
        arena
            .insert(parent, id, next)
            .map_err(mutation::link_error)?;
    }
    drop(state);
    node.at(id).wrap(context)
}

fn whitespace(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    Ok(Value::Bool(
        state.document.arena[node.id]
            .value
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')),
    ))
}

fn append_xml(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let target = this_node(this)?;
    if !target.shared.borrow().owned {
        return Err(dom_error(7, "No Modification Allowed Error"));
    }
    let data = phera_extension_api::args::bytes(args, 0);
    if data.is_empty() {
        return Ok(Value::Bool(true));
    }
    let mut wrapped = b"<fragment>".to_vec();
    wrapped.extend_from_slice(data);
    wrapped.extend_from_slice(b"</fragment>");
    let parsed = xml::parse(&wrapped, xml::Options::default());
    libxml::publish(
        context,
        "DOMDocumentFragment::appendXML",
        "",
        parsed.diagnostics,
        0,
    );
    let Some(document) = parsed.document else {
        return Ok(Value::Bool(false));
    };
    let Some(root) = document.arena[Arena::DOCUMENT].first_child() else {
        return Ok(Value::Bool(false));
    };
    let mut state = target.shared.borrow_mut();
    let arena = &mut state.document.arena;
    for child in document.arena.children(root) {
        let child = arena.import(&document.arena, child, true);
        arena
            .insert(target.id, child, None)
            .map_err(mutation::link_error)?;
    }
    Ok(Value::Bool(true))
}
