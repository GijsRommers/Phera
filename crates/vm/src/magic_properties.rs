//! `$object->name` calls __get only when normal lookup cannot read the slot.
//! A hook can access the same name again without calling itself recursively.

use std::{
    collections::HashSet,
    rc::{Rc, Weak},
};

use phera_extension_api::NativeResult;
use phera_value::{ArrayKey, Object, Value, to_bool};

use crate::Vm;
use crate::access_mode::Missing;
use crate::classes::property_hooks::HookRead;
use crate::classes::{Class, Method};
use crate::code_table::FunctionIndex;
use crate::path_update::property_name;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyHook {
    Get,
    Set,
    Isset,
    Unset,
}

pub(crate) struct UnsetProperties {
    object: Weak<Object>,
    keys: HashSet<ArrayKey>,
}

pub(crate) struct PropertyGuard {
    object: u32,
    name: ArrayKey,
    hook: PropertyHook,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropertyRead {
    Read,
    Update,
    Coalesce,
    Isset,
    Empty,
}

pub(crate) struct ReadProperty {
    pub(crate) value: Value,
    pub(crate) source: PropertyReadSource,
}

pub(crate) enum PropertyReadStep {
    Value(ReadProperty),
    Hook(FunctionIndex, Rc<Object>),
}

impl From<ReadProperty> for PropertyReadStep {
    fn from(value: ReadProperty) -> Self {
        Self::Value(value)
    }
}

#[derive(PartialEq, Eq)]
pub(crate) enum PropertyReadSource {
    Storage,
    Getter,
}

impl ReadProperty {
    fn stored(value: Value) -> Self {
        Self {
            value,
            source: PropertyReadSource::Storage,
        }
    }

    fn from_getter(value: Value) -> Self {
        Self {
            value,
            source: PropertyReadSource::Getter,
        }
    }
}

impl Vm {
    /// Returns the ordinary slot when present, or the accessible declaration
    /// governing a missing slot. Inaccessible declarations use the magic path.
    pub(crate) fn property_slot(
        &self,
        object: &Object,
        name: &str,
    ) -> (Option<Value>, Option<ArrayKey>) {
        match self.property_key(object, name) {
            Ok(key) => (crate::property_storage::read(object, &key), Some(key)),
            Err(_) => (None, None),
        }
    }

    /// A typed slot begins uninitialized, but explicit unset enables overloading.
    pub(crate) fn typed_property_uninitialized(
        &self,
        class: &Class,
        object: &Object,
        key: &ArrayKey,
    ) -> bool {
        class
            .defaults
            .iter()
            .any(|property| property.key == *key && property.ty.is_some())
            && !self
                .unset_typed_properties
                .get(&object.id)
                .is_some_and(|entry| {
                    std::ptr::eq(entry.object.as_ptr(), object) && entry.keys.contains(key)
                })
    }

    pub(crate) fn mark_typed_property_unset(&mut self, object: &Rc<Object>, key: ArrayKey) {
        let entry = self
            .unset_typed_properties
            .entry(object.id)
            .or_insert_with(|| UnsetProperties {
                object: Rc::downgrade(object),
                keys: HashSet::new(),
            });
        if !std::ptr::eq(entry.object.as_ptr(), &**object) {
            entry.object = Rc::downgrade(object);
            entry.keys.clear();
        }
        entry.keys.insert(key);
    }

    pub(crate) fn copy_unset_properties(&mut self, original: &Rc<Object>, cloned: &Rc<Object>) {
        if let Some(entry) = self.unset_typed_properties.get(&original.id)
            && std::ptr::eq(entry.object.as_ptr(), &**original)
        {
            let copy = UnsetProperties {
                object: Rc::downgrade(cloned),
                keys: entry.keys.clone(),
            };
            self.unset_typed_properties.insert(cloned.id, copy);
        }
    }

    pub(crate) fn property_hook(
        &self,
        class: &Class,
        object: &Object,
        key: &ArrayKey,
        hook: PropertyHook,
    ) -> Option<Rc<Method>> {
        let magic = class.magic.as_ref()?;
        let method = match hook {
            PropertyHook::Get => &magic.get,
            PropertyHook::Set => &magic.set,
            PropertyHook::Isset => &magic.isset,
            PropertyHook::Unset => &magic.unset,
        };
        method
            .as_ref()
            .filter(|_| {
                !self.property_guards.iter().any(|guard| {
                    guard.object == object.id && guard.hook == hook && guard.name == *key
                })
            })
            .cloned()
    }

    pub(crate) async fn invoke_property_hook(
        &mut self,
        class: Rc<Class>,
        object: Rc<Object>,
        hook: PropertyHook,
        key: ArrayKey,
        method: Rc<Method>,
        value: Option<Value>,
    ) -> NativeResult {
        let name = match &key {
            ArrayKey::String(name) => Value::String(name.clone()),
            ArrayKey::Int(index) => Value::from(index.to_string()),
        };
        self.property_guards.push(PropertyGuard {
            object: object.id,
            name: key,
            hook,
        });
        let result = self
            .invoke_magic_method(
                method,
                class,
                object,
                [Some(name), value].into_iter().flatten(),
            )
            .await;
        self.property_guards.pop();
        result
    }

    pub(crate) async fn plan_property_read(
        &mut self,
        value: Value,
        key: ArrayKey,
        mode: PropertyRead,
    ) -> Result<PropertyReadStep, phera_extension_api::NativeError> {
        let missing = match mode {
            PropertyRead::Read | PropertyRead::Update => Missing::Warn,
            PropertyRead::Coalesce => Missing::Silent,
            PropertyRead::Isset | PropertyRead::Empty => Missing::Isset,
        };
        let Value::Object(object) = &value else {
            return self
                .read_property(value, key, missing)
                .map(|value| ReadProperty::stored(value).into());
        };
        let name = property_name(&key);
        let native = if mode == PropertyRead::Update {
            crate::properties::native_property_access(object)
                .map(|access| {
                    (access.read)(
                        self,
                        object,
                        &name,
                        phera_extension_api::NativePropertyRead::Update,
                    )
                })
                .transpose()?
                .flatten()
        } else {
            self.native_property_read(object, &name, missing)?
        };
        if let Some(value) = native {
            return Ok(ReadProperty::stored(value).into());
        }
        let class = self.classes.for_object(object);
        let (stored, accessible) = self.property_slot(object, &name);
        if let Some(class) = &class
            && let Some(property) = accessible
                .as_ref()
                .and_then(|key| crate::classes::property_hooks::for_key(class, key))
            && let Some(hooks) = &property.hooks
        {
            match self.declared_hook_read(object, property, hooks, false)? {
                HookRead::Value(value) => return Ok(ReadProperty::stored(value).into()),
                HookRead::Getter(function) => {
                    return Ok(PropertyReadStep::Hook(function, object.clone()));
                }
                HookRead::Storage => {}
            }
            return self
                .read_declared_property(object, &name, missing)
                .map(|value| ReadProperty::stored(value).into());
        }
        let Some(class) = class.filter(|class| {
            class
                .magic
                .as_ref()
                .is_some_and(|hooks| hooks.has_properties())
        }) else {
            return self
                .read_declared_property(object, &name, missing)
                .map(|value| ReadProperty::stored(value).into());
        };
        if let Some(stored) = stored {
            return Ok(ReadProperty::stored(stored).into());
        }
        if accessible
            .as_ref()
            .is_some_and(|slot| self.typed_property_uninitialized(&class, object, slot))
        {
            return self
                .read_declared_property(object, &name, missing)
                .map(|value| ReadProperty::stored(value).into());
        }
        if !matches!(mode, PropertyRead::Read | PropertyRead::Update) {
            if let Some(method) = self.property_hook(&class, object, &key, PropertyHook::Isset) {
                let exists = self
                    .invoke_property_hook(
                        class.clone(),
                        object.clone(),
                        PropertyHook::Isset,
                        key.clone(),
                        method,
                        None,
                    )
                    .await?;
                if !to_bool(&exists) {
                    return Ok(ReadProperty::stored(Value::Null).into());
                }
                if mode == PropertyRead::Isset {
                    return Ok(ReadProperty::stored(Value::Bool(true)).into());
                }
            } else if mode != PropertyRead::Coalesce {
                return Ok(ReadProperty::stored(Value::Null).into());
            }
        }
        let Some(method) = self.property_hook(&class, object, &key, PropertyHook::Get) else {
            return self
                .read_declared_property(object, &name, missing)
                .map(|value| ReadProperty::stored(value).into());
        };
        let result = self
            .invoke_property_hook(
                class.clone(),
                object.clone(),
                PropertyHook::Get,
                key,
                method,
                None,
            )
            .await?;
        self.validate_getter_value(&class, accessible.as_ref(), result)
            .await
            .map(|value| ReadProperty::from_getter(value).into())
    }
}
