//! Indexed children and attribute access on SimpleXML selections.
//!
//! ```php
//! $xml->item[] = 'second';
//! $xml->item[0]['id'] = 'first';
//! ```

use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Kind, Name};
use phera_extension_api::{
    Context, DimensionReferenceMode, NativeDimensions, NativeError, NativeResult, Object, Value,
    value::to_php_string,
};

use crate::properties::{add_attribute, append_element, assigned_value, materialize};
use crate::selection::{Selected, Selection, State, attributes, data, wrap};

pub const ACCESS: NativeDimensions = NativeDimensions {
    fallback_if_overridden: &[],
    get: dimension,
    exists: dimension_exists,
    set: set_dimension,
    unset: unset_dimension,
    reference: Some(dimension_for_mutation),
    bind: None,
    direct: None,
};

enum Offset {
    Index(i64),
    Append,
    Name(String),
}

fn dimension_for_mutation(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
    mode: DimensionReferenceMode,
) -> NativeResult {
    let state = data(object)?;
    let selection = &state.selection;
    let key = offset(context, &key)?;
    let node = match key {
        key if mode == DimensionReferenceMode::Unset => {
            materialize(&state)?;
            indexed(selection, key)
        }
        Offset::Index(index) => target_element(context, &state, Some(index))?,
        Offset::Append => target_element(context, &state, None)?,
        name => {
            materialize(&state)?;
            indexed(selection, name)
        }
    };
    match node {
        Some(node) => wrap(
            context,
            object.class.name(),
            Selection::node(node, selection.filter.clone()),
        ),
        None => Ok(Value::Null),
    }
}

fn offset(context: &mut dyn Context, value: &Value) -> Result<Offset, NativeError> {
    match &*value.deref() {
        Value::Int(index) => Ok(Offset::Index(*index)),
        Value::Null => Ok(Offset::Append),
        value => Ok(Offset::Name(
            to_php_string(value, context.diagnostics())?.to_string_lossy(),
        )),
    }
}

fn indexed(selection: &Selection, offset: Offset) -> Option<SharedNode> {
    match offset {
        Offset::Index(index) => {
            let index = usize::try_from(index).ok()?;
            let node = selection
                .anchor
                .document(|doc| selection.nodes_in(&doc.arena).get(index).copied())?;
            selection.anchor.select(node)
        }
        Offset::Name(name) => {
            if matches!(&selection.kind, Selected::Attributes(Some(expected)) if expected != &name)
            {
                return None;
            }
            let owner = selection.attribute_owner()?;
            let node = owner.document(|doc| {
                attributes(&doc.arena, owner.id(), &selection.filter, Some(&name))
                    .first()
                    .copied()
            })?;
            owner.select(node)
        }
        Offset::Append => None,
    }
}

pub(crate) fn dimension(context: &mut dyn Context, object: &Object, key: Value) -> NativeResult {
    let state = data(object)?;
    let key = offset(context, &key)?;
    match indexed(&state.selection, key) {
        Some(node) => wrap(
            context,
            object.class.name(),
            Selection::node(node, state.selection.filter.clone()),
        ),
        None => Ok(Value::Null),
    }
}

pub(crate) fn dimension_exists(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
) -> Result<bool, NativeError> {
    let state = data(object)?;
    Ok(indexed(&state.selection, offset(context, &key)?).is_some())
}

pub(crate) fn set_dimension(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
    value: Value,
) -> Result<(), NativeError> {
    let state = data(object)?;
    let selection = &state.selection;
    let key = offset(context, &key)?;
    if matches!(&key, Offset::Name(name) if name.trim().is_empty()) {
        return Err(NativeError::value_error(
            "Cannot create attribute with an empty name",
        ));
    }
    let target = if matches!(key, Offset::Name(_)) {
        "attributes"
    } else {
        "properties"
    };
    let value = assigned_value(context, &value, target)?;
    match key {
        Offset::Name(name) => {
            let name = name.trim().to_owned();
            let owner = match (selection.attribute_owner(), &selection.kind) {
                (None, Selected::Children(Some(element))) => Some(append_element(
                    &selection.anchor,
                    element,
                    &selection.filter,
                )?),
                (owner, _) => owner,
            };
            if let Some(owner) = owner {
                if let Some(attribute) = indexed(selection, Offset::Name(name.clone())) {
                    attribute.assign_text(value);
                } else {
                    add_attribute(&owner, Name::new(name), value)?;
                }
            }
        }
        Offset::Index(index) => {
            if let Some(node) = target_element(context, &state, Some(index))? {
                node.assign_text(value);
            }
        }
        Offset::Append => {
            if let Some(node) = target_element(context, &state, None)? {
                node.assign_text(value);
            } else {
                return Err(NativeError::value_error(
                    "Cannot append to an attribute list",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn unset_dimension(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
) -> Result<(), NativeError> {
    let state = data(object)?;
    materialize(&state)?;
    if let Some(node) = indexed(&state.selection, offset(context, &key)?) {
        node.remove(node.id());
    }
    Ok(())
}

fn target_element(
    context: &mut dyn Context,
    state: &State,
    index: Option<i64>,
) -> Result<Option<SharedNode>, NativeError> {
    let selection = &state.selection;
    if let Some(index) = index
        && let Some(node) = indexed(selection, Offset::Index(index))
    {
        return Ok(Some(node));
    }
    let (name, owner, count) = match &selection.kind {
        Selected::Children(Some(name)) => {
            let count = selection.items().len().try_into().unwrap_or(i64::MAX);
            (name.clone(), Some(selection.anchor.clone()), count)
        }
        Selected::Children(None) | Selected::Node => {
            let Some(first) = selection.first() else {
                return Ok(None);
            };
            let (name, parent) = first.document(|doc| {
                let node = &doc.arena[first.id()];
                let parent = node
                    .parent()
                    .filter(|&id| doc.arena[id].kind == Kind::Element);
                (node.name.local.clone(), parent)
            });
            if matches!(selection.kind, Selected::Node) {
                (name, parent.and_then(|id| first.select(id)), 0)
            } else {
                let count = selection.items().len().try_into().unwrap_or(i64::MAX);
                (name, Some(selection.anchor.clone()), count)
            }
        }
        Selected::Attributes(_) => {
            if let Some(index) = index {
                let count = if index < 0 {
                    0
                } else {
                    selection.items().len()
                };
                context.warning(format!(
                    "Cannot change attribute number {index} when only {count} attributes exist"
                ));
            }
            return Ok(None);
        }
    };
    if let Some(index) = index {
        if index < 0 || index > count || matches!(selection.kind, Selected::Node) {
            if count == 0 {
                materialize(state)?;
            }
            context.warning(format!(
                "Cannot add element {name} number {index} when only {count} such elements exist"
            ));
        }
        if index < 0 || (count == 0 && index > 0) || matches!(selection.kind, Selected::Node) {
            return Ok(None);
        }
    }
    owner
        .map(|owner| append_element(&owner, &name, &selection.filter))
        .transpose()
}
