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
    Context, NativeError, NativePropertyAccess, NativePropertyMutation, NativePropertyRead, Object,
    Value,
    value::{ArrayKey, to_php_string},
};

use crate::selection::{Filter, Selected, Selection, State, data, wrap, wrap_for_mutation};

pub const ACCESS: NativePropertyAccess = NativePropertyAccess::new(read_property, mutate_property);

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
    if matches!(mode, NativePropertyRead::Update) {
        materialize(&state)?;
    }
    if matches!(state.selection.kind, Selected::Attributes(_)) {
        let key = Value::from(name);
        return match mode {
            NativePropertyRead::Isset | NativePropertyRead::Exists => {
                Ok(crate::dimensions::dimension_exists(context, object, key)?
                    .then_some(Value::Bool(true)))
            }
            _ => Ok(Some(crate::dimensions::dimension(context, object, key)?)),
        };
    }
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
    if matches!(mode, NativePropertyRead::Quiet) && selection.first().is_none() {
        return Ok(Some(Value::Null));
    }
    if matches!(mode, NativePropertyRead::Update) {
        return Ok(Some(wrap_for_mutation(
            context,
            object.class.name(),
            selection,
        )?));
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
    if matches!(state.selection.kind, Selected::Attributes(_)) {
        let key = Value::from(name);
        return match mutation {
            NativePropertyMutation::Assign(value) => {
                crate::dimensions::set_dimension(context, object, key, value.clone())?;
                Ok(true)
            }
            NativePropertyMutation::Unset => {
                crate::dimensions::unset_dimension(context, object, key)?;
                Ok(true)
            }
            _ => Ok(false),
        };
    }
    if matches!(
        mutation,
        NativePropertyMutation::Assign(_) | NativePropertyMutation::Unset
    ) {
        materialize(&state)?;
    }
    match mutation {
        NativePropertyMutation::Assign(value) => {
            let name = name.trim();
            if name.is_empty() {
                return Err(NativeError::value_error(
                    "Cannot create element with an empty name",
                ));
            }
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

pub fn materialize(state: &State) -> Result<(), NativeError> {
    let selection = &state.selection;
    if state.materialize_on_mutation
        && let Selected::Children(Some(name)) = &selection.kind
        && selection.first().is_none()
    {
        append_element(&selection.anchor, name, &selection.filter)?;
    }
    Ok(())
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

pub fn assigned_value(
    context: &mut dyn Context,
    value: &Value,
    target: &str,
) -> Result<String, NativeError> {
    if let Value::Object(object) = &*value.deref()
        && object.class.is_instance_of(crate::selection::CLASS)
    {
        return Ok(data(object)?.selection.value());
    }
    if value.is_object() || value.is_array() || matches!(&*value.deref(), Value::Resource(_)) {
        return Err(NativeError::type_error(format!(
            "It's not possible to assign a complex type to {target}, {} given",
            value.type_name(),
        )));
    }
    Ok(to_php_string(value, context.diagnostics())?.to_string_lossy())
}
