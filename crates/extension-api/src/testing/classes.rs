//! Creating objects of native classes and calling their methods without a
//! VM. This is also the reference for what the VM does with a
//! [`ClassEntry`]: properties are laid out root class first, private ones
//! under their mangled key, and a method is looked up from the object's
//! class upwards.

use std::any::Any;
use std::rc::Rc;

use phera_value::{ArrayKey, ClassHandle, Object, Value};

use super::TestContext;
use crate::{
    ClassEntry, MethodEntry, NativeError, NativeMethod, NativeResult, Registry, Visibility,
    prepare_call,
};

/// Just enough of a class for an object to name its own.
pub(super) struct NamedClass(pub(super) String);

impl ClassHandle for NamedClass {
    fn name(&self) -> &str {
        &self.0
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// The class and its ancestors, the class itself first.
fn lineage<'a>(registry: &'a Registry, class: &str) -> Vec<&'a ClassEntry> {
    let mut chain = Vec::new();
    let mut next = registry.get_class(class);
    while let Some(entry) = next {
        chain.push(entry);
        next = entry.parent.and_then(|parent| registry.get_class(parent));
    }
    chain
}

/// A new object with every declared property at its default. The constructor
/// is not called; use [`call_method`] with `__construct` for that.
pub fn instantiate(registry: &Registry, class: &str) -> Option<Rc<Object>> {
    let chain = lineage(registry, class);
    let entry = chain.first()?;
    let object = Object::new(1, Rc::new(NamedClass(entry.name.to_string())));
    for ancestor in chain.iter().rev() {
        for property in ancestor
            .properties
            .iter()
            .filter(|property| !property.is_static)
        {
            let key = match property.visibility {
                Visibility::Private => Object::private_key(ancestor.name, property.name),
                Visibility::Protected => Object::protected_key(property.name),
                Visibility::Public => ArrayKey::str(property.name),
            };
            if let Some(default) = property.default {
                object.set(key, default.to_value());
            }
        }
    }
    Some(Rc::new(object))
}

/// Calls a sync method the way the VM does: found on the object's class or
/// the nearest ancestor that defines it, arguments prepared first.
pub fn call_method(
    registry: &Registry,
    context: &mut TestContext,
    object: &Rc<Object>,
    method: &str,
    mut arguments: Vec<Value>,
) -> NativeResult {
    let class = object.class.name();
    let found: Option<(&ClassEntry, &MethodEntry)> =
        lineage(registry, class).into_iter().find_map(|entry| {
            entry
                .methods
                .iter()
                .find(|candidate| candidate.name.eq_ignore_ascii_case(method))
                .map(|candidate| (entry, candidate))
        });
    let Some((declaring, entry)) = found else {
        return Err(NativeError::error(format!(
            "Call to undefined method {class}::{method}()"
        )));
    };
    let name = format!("{}::{}", declaring.name, entry.name);
    prepare_call(&name, entry.params, &mut arguments, context)?;
    match entry.handler {
        NativeMethod::Sync(function) => function(context, Some(object), &arguments),
        NativeMethod::Async(_)
        | NativeMethod::AsyncNamed(_)
        | NativeMethod::Window(_)
        | NativeMethod::Abstract => Err(NativeError::error(format!(
            "{name}() cannot be called from testing::call_method"
        ))),
    }
}
