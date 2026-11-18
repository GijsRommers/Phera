//! `ReflectionProperty::setValue()` writes through the declared key even when it is private.

use std::rc::Rc;

use phera_extension_api::reflection::{MemberInfo, PropertyRead};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Object, Value};

use crate::{Vm, classes::class::PropertyDefault};

use super::exception;

impl Vm {
    pub(super) fn reflection_property_read(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        mode: PropertyRead,
    ) -> NativeResult {
        let property = self.reflection_property(member)?;
        if matches!(mode, PropertyRead::Default) {
            return self.reflection_property_default(&property);
        }
        let value = match &property.static_storage {
            Some(storage) => {
                if !property.static_initialized.get() {
                    let initial = self.reflection_property_default(&property)?;
                    *storage.reference.borrow().borrow_mut() = initial;
                    property.static_initialized.set(true);
                }
                property
                    .value_initialized
                    .get()
                    .then(|| storage.reference.borrow().borrow().clone())
            }
            None => {
                let object = property_object(&property, object)?;
                crate::property_storage::read(&object, &property.key)
            }
        };
        if matches!(mode, PropertyRead::Initialized) {
            return Ok(Value::Bool(value.is_some()));
        }
        value.map(|value| value.deref().clone()).map_or_else(
            || {
                if property.ty.is_some() {
                    Err(NativeError::error(format!(
                        "Typed property {}::${} must not be accessed before initialization",
                        property.declaring, property.name
                    )))
                } else {
                    Ok(Value::Null)
                }
            },
            Ok,
        )
    }

    pub(super) fn reflection_property_write(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        value: Value,
    ) -> NativeResult {
        if let Some(object) = &object
            && !object.class.is_instance_of(&member.class)
            && member.modifiers & 16 == 0
        {
            return phera_extension_api::reflection::ReflectionContext::reflect_dynamic_property_write(
                self, object.clone(), &member.name, value,
            );
        }
        let property = self.reflection_property(member)?;
        let value = value.deref().clone();
        let value = match &property.ty {
            Some(ty) => self.coerce_reflected_property(value, &property, ty)?,
            None => value,
        };
        if let Some(storage) = &property.static_storage {
            *storage.reference.borrow().borrow_mut() = value;
            property.static_initialized.set(true);
            property.value_initialized.set(true);
        } else {
            let object = property_object(&property, object)?;
            if property.is_readonly && object.has(&property.key) {
                return Err(NativeError::error(format!(
                    "Cannot modify readonly property {}::${}",
                    property.declaring, property.name
                )));
            }
            crate::property_storage::write(&object, property.key, value);
        }
        Ok(Value::Null)
    }

    pub(super) fn reflection_property(
        &self,
        member: &MemberInfo,
    ) -> Result<PropertyDefault, NativeError> {
        self.classes
            .get(&member.class)
            .and_then(|class| {
                class
                    .defaults
                    .iter()
                    .chain(&class.static_properties)
                    .find(|property| {
                        property.name == member.name && property.declaring == member.class
                    })
                    .cloned()
            })
            .ok_or_else(|| {
                exception(format!(
                    "Property {}::${} does not exist",
                    member.class, member.name
                ))
            })
    }

    fn reflection_property_default(&mut self, property: &PropertyDefault) -> NativeResult {
        if let Some(initializer) = &property.initializer {
            return self.evaluate_initializer(initializer, &property.declaring);
        }
        Ok(property.value.deref().clone())
    }
}

fn property_object(
    property: &PropertyDefault,
    object: Option<Rc<Object>>,
) -> Result<Rc<Object>, NativeError> {
    let object = object.ok_or_else(|| NativeError::type_error(
        "ReflectionProperty::getValue(): Argument #1 ($object) must be provided for instance properties"
    ))?;
    if !object.class.is_instance_of(&property.declaring) {
        return Err(exception(
            "Given object is not an instance of the class this property was declared in",
        ));
    }
    Ok(object)
}
