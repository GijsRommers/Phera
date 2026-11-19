//! Access named children, indexed siblings and attributes through PHP syntax.
//!
//! ```php
//! $xml->item = 'updated';
//! $xml->item['active'] = 'yes';
//! echo $xml->item[0], $xml->item['active'];
//! ```

use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Kind, Name, Node, NodeId};
use phera_extension_api::{
    Context, NativeDimensions, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, NativeResult, Object, Value,
    value::{ArrayKey, to_php_string},
};

use crate::selection::{Filter, Selected, Selection, attributes, data, wrap};

pub const ACCESS: NativePropertyAccess = NativePropertyAccess {
    read: read_property,
    mutate: mutate_property,
};

pub const DIMENSIONS: NativeDimensions = NativeDimensions {
    fallback_if_overridden: &[],
    get: dimension,
    exists: dimension_exists,
    set: set_dimension,
    unset: unset_dimension,
    reference: None,
    bind: None,
    direct: None,
};

fn read_property(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if object.class.has_property(name) || object.get(&ArrayKey::str(name)).is_some() {
        return Ok(None);
    }
    let state = data(object)?;
    let Some(selection) = state.selection.for_property(name.to_owned()) else {
        return Ok(
            if matches!(mode, NativePropertyRead::Exists | NativePropertyRead::Isset) {
                None
            } else {
                Some(Value::Null)
            },
        );
    };
    if matches!(mode, NativePropertyRead::Isset | NativePropertyRead::Exists) {
        return Ok(selection.first().map(|_| Value::Bool(true)));
    }
    Ok(Some(wrap(context, object.class.name(), selection)?))
}

fn mutate_property(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if object.class.has_property(name) || object.get(&ArrayKey::str(name)).is_some() {
        return Ok(false);
    }
    let state = data(object)?;
    match mutation {
        NativePropertyMutation::Assign(value) => {
            if let Some(selection) = state.selection.for_property(name.to_owned()) {
                let value = assigned_value(context, value, "properties")?;
                assign_child(context, &selection, name, value)?;
            }
            Ok(true)
        }
        NativePropertyMutation::Unset => {
            if let Some(selection) = state.selection.for_property(name.to_owned()) {
                let nodes = selection
                    .anchor
                    .document(|doc| selection.nodes_in(&doc.arena));
                for node in nodes {
                    selection.anchor.remove(node);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn assign_child(
    context: &mut dyn Context,
    selection: &Selection,
    name: &str,
    value: String,
) -> Result<(), NativeError> {
    let children = selection
        .anchor
        .document(|doc| selection.nodes_in(&doc.arena));
    match children.as_slice() {
        [] => {
            let node = append_element(&selection.anchor, name, &selection.filter)?;
            node.assign_text(value);
        }
        [node] => {
            if let Some(node) = selection.anchor.select(*node) {
                node.assign_text(value);
            }
        }
        _ => context.warning(
            "Cannot assign to an array of nodes (duplicate subnodes or attr detected)".into(),
        ),
    }
    Ok(())
}

pub fn append_element(
    owner: &SharedNode,
    name: &str,
    filter: &Filter,
) -> Result<SharedNode, NativeError> {
    if !phera_ext_dom::xml::valid_name(name) {
        return Err(NativeError::error("Cannot create unnamed element"));
    }
    let element = owner.document_mut(|doc| {
        let arena = &mut doc.arena;
        let ancestor = &arena[owner.id()].name;
        let mut name = Name::new(name);
        match filter {
            Filter::Default => {
                name.namespace.clone_from(&ancestor.namespace);
                name.prefix.clone_from(&ancestor.prefix);
            }
            Filter::Prefix(prefix) => {
                name.prefix = Some(prefix.clone());
                name.namespace = super::namespaces::lookup(arena, owner.id(), prefix);
            }
            Filter::Namespace(namespace) => {
                name.namespace = Some(namespace.clone());
                if ancestor.namespace.as_ref() == Some(namespace) {
                    name.prefix = ancestor.prefix.clone();
                }
            }
        }
        let element = arena.allocate(Node::new(Kind::Element, name, ""));
        arena
            .insert(owner.id(), element, None)
            .ok()
            .map(|_| element)
    });
    element
        .and_then(|element| owner.select(element))
        .ok_or_else(|| {
            NativeError::error("Cannot add child. Parent is not a permanent member of the XML tree")
        })
}

enum Offset {
    Index(i64),
    Append,
    Name(String),
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

fn dimension(context: &mut dyn Context, object: &Object, key: Value) -> NativeResult {
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

fn dimension_exists(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
) -> Result<bool, NativeError> {
    let state = data(object)?;
    Ok(indexed(&state.selection, offset(context, &key)?).is_some())
}

fn set_dimension(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
    value: Value,
) -> Result<(), NativeError> {
    let state = data(object)?;
    let selection = &state.selection;
    let key = offset(context, &key)?;
    let target = if matches!(key, Offset::Name(_)) {
        "attributes"
    } else {
        "properties"
    };
    let value = assigned_value(context, &value, target)?;
    match key {
        Offset::Name(name) => {
            if let Some(owner) = selection.attribute_owner() {
                if let Some(attribute) = indexed(selection, Offset::Name(name.clone())) {
                    attribute.assign_text(value);
                } else {
                    add_attribute(&owner, Name::new(name), value)?;
                }
            }
        }
        Offset::Index(index) => {
            if let Some(node) = indexed(selection, Offset::Index(index)) {
                node.assign_text(value);
            } else if let Selected::Children(Some(name)) = &selection.kind
                && i64::try_from(selection.items().len()).ok() == Some(index)
            {
                append_element(&selection.anchor, name, &selection.filter)?.assign_text(value);
            }
        }
        Offset::Append => {
            if let Selected::Children(Some(name)) = &selection.kind {
                append_element(&selection.anchor, name, &selection.filter)?.assign_text(value);
            } else {
                return Err(NativeError::error("Cannot create unnamed attribute"));
            }
        }
    }
    Ok(())
}

fn unset_dimension(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
) -> Result<(), NativeError> {
    let state = data(object)?;
    if let Some(node) = indexed(&state.selection, offset(context, &key)?) {
        node.remove(node.id());
    }
    Ok(())
}

pub fn add_attribute(
    owner: &SharedNode,
    name: Name,
    value: String,
) -> Result<Option<NodeId>, NativeError> {
    let node = owner.document_mut(|doc| {
        let arena = &mut doc.arena;
        if arena[owner.id()].kind != Kind::Element {
            return Ok(None);
        }
        if !phera_ext_dom::xml::valid_name(&name.qualified()) {
            return Err(NativeError::error("Cannot create unnamed attribute"));
        }
        let id = arena.allocate(Node::new(Kind::Attribute, name, ""));
        arena
            .set_attribute(owner.id(), id, None)
            .map_err(|_| NativeError::error("Unable to set attribute"))?;
        Ok(Some(id))
    })?;
    if let Some(attribute) = node.and_then(|id| owner.select(id)) {
        attribute.assign_text(value);
    }
    Ok(node)
}

fn assigned_value(
    context: &mut dyn Context,
    value: &Value,
    target: &str,
) -> Result<String, NativeError> {
    if let Value::Object(object) = &*value.deref()
        && object.class.is_instance_of(crate::selection::CLASS)
    {
        return Ok(data(object)?.selection.value());
    }
    if value.is_object() || value.is_array() {
        return Err(NativeError::type_error(format!(
            "It's not possible to assign a complex type to {target}, {} given",
            value.type_name(),
        )));
    }
    Ok(to_php_string(value, context.diagnostics())?.to_string_lossy())
}
