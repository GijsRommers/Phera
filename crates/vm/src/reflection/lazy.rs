//! `ReflectionProperty(Foo::class, 'id')->skipLazyInitialization($ghost)`
//! restores the default for that property only.

use std::rc::Rc;

use phera_extension_api::NativeResult;
use phera_extension_api::reflection::lazy::{LazyClassQuery, LazyPropertyOperation};
use phera_extension_api::reflection::{MemberInfo, PropertyWrite};
use phera_value::{LazyObject, Object, Value};

use crate::Vm;
use crate::lazy_continuation::ReflectInitialize;
use crate::lazy_objects::resolved_lazy_instance;

use super::{exception, properties::property_at};

impl Vm {
    pub(super) fn reflection_lazy_object(
        &mut self,
        object: Rc<Object>,
        query: LazyClassQuery,
    ) -> NativeResult {
        match query {
            LazyClassQuery::IsUninitialized => Ok(Value::Bool(object.is_uninitialized_lazy())),
            LazyClassQuery::GetInitializer => Ok(match object.lazy().as_deref() {
                Some(LazyObject::Pending { initializer, .. }) => initializer.clone(),
                _ => Value::Null,
            }),
            LazyClassQuery::Initialize if object.is_uninitialized_lazy() => {
                Err(ReflectInitialize(object).suspend())
            }
            LazyClassQuery::MarkInitialized if object.is_uninitialized_lazy() => {
                let pending = object.replace_lazy(None);
                if let Some(LazyObject::Pending { properties, .. }) = pending.as_deref()
                    && let Err(error) = self.restore_lazy_defaults(&object, properties)
                {
                    object.replace_lazy(pending);
                    return Err(error);
                }
                Ok(Value::Object(object))
            }
            _ => Ok(Value::Object(object.lazy_instance().unwrap_or(object))),
        }
    }

    pub(super) fn reflection_lazy_property(
        &mut self,
        member: &MemberInfo,
        object: Rc<Object>,
        operation: LazyPropertyOperation,
    ) -> NativeResult {
        let object = resolved_lazy_instance(object);
        let property = if member.dynamic {
            None
        } else {
            let (class, slot) = self.reflected_property_slot(member)?;
            property_at(&class, slot)
                .map(|property| Self::effective_property(property, slot, &object).0.clone())
        };
        let category = match &property {
            None => Some("dynamic"),
            Some(property) if property.static_storage.is_some() => Some("static"),
            Some(property)
                if property
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.hooks.is_virtual) =>
            {
                Some("virtual")
            }
            _ => None,
        };
        if let Some(category) = category {
            return if matches!(operation, LazyPropertyOperation::IsLazy) {
                Ok(Value::Bool(false))
            } else {
                Err(exception(format!(
                    "Can not use {} on {category} property {}::${}",
                    operation.method(),
                    property
                        .as_ref()
                        .map_or(member.class.as_str(), |property| property
                            .declaring
                            .as_str()),
                    member.name,
                )))
            };
        }
        let Some(property) = property else {
            return Err(internal_error!("reflected lazy property"));
        };
        let is_lazy = matches!(object.lazy().as_deref(), Some(LazyObject::Pending { properties, .. }) if properties.contains(&property.key));
        match operation {
            LazyPropertyOperation::IsLazy => return Ok(Value::Bool(is_lazy)),
            LazyPropertyOperation::Skip if is_lazy => {
                if let Some(default) = self.lazy_property_default(&property)? {
                    object.set(property.key.clone(), default);
                }
            }
            LazyPropertyOperation::Skip => return Ok(Value::Null),
            LazyPropertyOperation::Write(value) => {
                self.reflection_property_write(
                    member,
                    Some(object.clone()),
                    value,
                    PropertyWrite::Raw,
                )?;
            }
        }
        if is_lazy {
            let mut state = object.replace_lazy(None);
            if let Some(LazyObject::Pending { properties, .. }) = state.as_deref_mut() {
                properties.remove(&property.key);
                if !properties.is_empty() {
                    object.replace_lazy(state);
                }
            }
        }
        Ok(Value::Null)
    }
}
