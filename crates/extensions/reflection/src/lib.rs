//! PHP introspection, such as `(new ReflectionClass(Service::class))->getConstructor()`.

use std::any::Any;
use std::rc::Rc;

use phera_extension_api::reflection::ReflectionContext;
use phera_extension_api::{
    ArrayKey, ClassEntry, Context, Extension, NativeClone, NativeError, NativeResult, Object,
    PhpArray, Registry, Value,
};

mod attribute;
mod builtins;
mod class;
mod class_members;
mod constant;
mod enum_reflection;
mod extension;
mod fork;
mod function;
mod generator;
mod lazy;
mod member;
mod method;
mod modifiers;
mod parameter;
mod property;
mod reference;
mod types;

pub struct Reflection;

impl Extension for Reflection {
    fn name(&self) -> &'static str {
        "Reflection"
    }

    fn register(&self, registry: &mut Registry) {
        // How a fork of a snapshot gets this extension's request state.
        registry.fork_state::<crate::reference::ReferenceIds>(phera_extension_api::fresh_state);
        builtins::register(registry);
        registry.class(ClassEntry {
            parent: Some("Exception"),
            ..ClassEntry::class("ReflectionException")
        });
        registry.class(ClassEntry {
            interfaces: &["Stringable"],
            ..ClassEntry::interface("Reflector")
        });
        registry.class(ClassEntry {
            kind: phera_extension_api::ClassKind::UnitEnum(&["Get", "Set"]),
            ..ClassEntry::class("PropertyHookType")
        });
        modifiers::register(registry);
        class::register(registry);
        member::register(registry);
        enum_reflection::register(registry);
        function::register(registry);
        generator::register(registry);
        parameter::register(registry);
        types::register(registry);
        extension::register(registry);
        attribute::register(registry);
        reference::register(registry);
        constant::register(registry);
    }
}

static UNCLONEABLE: phera_extension_api::ClassHooks = phera_extension_api::ClassHooks {
    clone: Some(NativeClone::Forbidden),
    properties: Some(reflected_properties),
    fork: Some(fork::fork_state),
    ..phera_extension_api::ClassHooks::EMPTY
};

// PHP values live in normal object storage so iterative object teardown can reach them.
const RETAINED: &[u8] = b"\0Reflection\0retained";

fn keep(object: &Object, value: Value) {
    object.set(ArrayKey::str(RETAINED), value);
}

fn reflected_properties(
    _: &mut dyn Context,
    object: &Object,
    _: phera_extension_api::PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut properties = object.to_array();
    properties.remove(&ArrayKey::str(RETAINED));
    Ok(properties)
}

fn engine(context: &mut dyn Context) -> Result<&mut dyn ReflectionContext, NativeError> {
    context
        .reflection()
        .ok_or_else(|| NativeError::error("Reflection requires the PHP runtime"))
}

fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("ReflectionException", message)
}

fn receiver(this: Option<&Rc<Object>>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Reflection requires an object"))
}

fn load<T: Any + Clone>(this: Option<&Rc<Object>>) -> Result<T, NativeError> {
    receiver(this)?
        .native
        .try_borrow()
        .map_err(|_| NativeError::error("Reflection object is already in use"))?
        .as_ref()
        .and_then(|state| state.downcast_ref::<T>())
        .cloned()
        .ok_or_else(|| {
            NativeError::error("Internal error: Failed to retrieve the reflection object")
        })
}

fn bind<T: Any>(object: &Object, name: Option<&str>, value: T) -> NativeResult {
    if let Some(name) = name {
        object.set(ArrayKey::str("name"), Value::from(name));
    }
    *object
        .native
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Reflection object is already in use"))? =
        Some(Box::new(value));
    Ok(Value::Null)
}

fn allocate<T: Any>(
    context: &mut dyn Context,
    class: &str,
    name: Option<&str>,
    state: T,
) -> NativeResult {
    let value = context.create_object(class, PhpArray::new())?;
    let Value::Object(object) = &value else {
        return Err(NativeError::error("Reflection object allocation failed"));
    };
    bind(object, name, state)?;
    Ok(value)
}

fn values(items: impl IntoIterator<Item = Value>) -> NativeResult {
    let mut result = PhpArray::new();
    for item in items {
        result.push(item)?;
    }
    Ok(Value::from(result))
}
