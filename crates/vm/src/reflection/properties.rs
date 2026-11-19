//! `ReflectionProperty::setValue()` writes through the declared key even when it is private.

use std::rc::Rc;

use phera_extension_api::reflection::{MemberInfo, PropertyRead, PropertySlot};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Object, Value};

use crate::{
    Vm,
    classes::{Class, class::PropertyDefault},
};

use super::exception;

impl Vm {
    pub(super) fn reflection_property_read(
        &mut self,
        member: &MemberInfo,
        object: Option<Rc<Object>>,
        mode: PropertyRead,
    ) -> NativeResult {
        let (class, slot) = self.reflected_property_slot(member)?;
        let property = property_at(&class, slot).ok_or_else(|| missing_property(member))?;
        if matches!(mode, PropertyRead::Default) {
            return self.reflection_property_default(property);
        }
        let value = match &property.static_storage {
            Some(storage) => {
                if !property.static_initialized.get() {
                    let initial = self.reflection_property_default(property)?;
                    *storage.reference.borrow().borrow_mut() = initial;
                    property.static_initialized.set(true);
                }
                property
                    .value_initialized
                    .get()
                    .then(|| storage.reference.borrow().borrow().clone())
            }
            None => {
                let object = property_object(property, object)?;
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
        let (class, slot) = self.reflected_property_slot(member)?;
        let property = property_at(&class, slot).ok_or_else(|| missing_property(member))?;
        let value = value.deref().clone();
        let value = match &property.ty {
            Some(ty) => self.coerce_property_in_mode(value, property, ty, Some(false))?,
            None => value,
        };
        if let Some(storage) = &property.static_storage {
            let reference = storage.reference.borrow().clone();
            let converted = self.check_reference_assignment(&reference, value)?;
            *reference
                .try_borrow_mut()
                .map_err(|_| exception("Property value is already in use"))? = converted;
            property.static_initialized.set(true);
            property.value_initialized.set(true);
        } else {
            let object = property_object(property, object)?;
            if property.is_readonly && object.has(&property.key) {
                return Err(NativeError::error(format!(
                    "Cannot modify readonly property {}::${}",
                    property.declaring, property.name
                )));
            }
            if let Some(Value::Reference(reference)) =
                crate::property_storage::read(&object, &property.key)
            {
                let converted = self.check_reference_assignment(&reference, value)?;
                *reference
                    .try_borrow_mut()
                    .map_err(|_| exception("Property value is already in use"))? = converted;
            } else {
                crate::property_storage::write(&object, property.key.clone(), value);
            }
        }
        Ok(Value::Null)
    }

    fn reflected_property_slot(
        &self,
        member: &MemberInfo,
    ) -> Result<(Rc<Class>, PropertySlot), NativeError> {
        if let Some(handle) = member.property {
            let class = self
                .classes
                .numbered(handle.class)
                .ok_or_else(|| missing_property(member))?;
            return Ok((class, handle.slot));
        }
        let class = self
            .classes
            .get(&member.class)
            .ok_or_else(|| missing_property(member))?;
        let matches = |property: &PropertyDefault| {
            property.name == member.name && property.declaring == member.class
        };
        let slot = class
            .defaults
            .iter()
            .position(matches)
            .map(PropertySlot::Instance)
            .or_else(|| {
                class
                    .static_properties
                    .iter()
                    .position(matches)
                    .map(PropertySlot::Static)
            })
            .ok_or_else(|| missing_property(member))?;
        Ok((class, slot))
    }

    fn reflection_property_default(&mut self, property: &PropertyDefault) -> NativeResult {
        if let Some(initializer) = &property.initializer {
            return self.evaluate_initializer(initializer, &property.declaring);
        }
        Ok(property.value.deref().clone())
    }
}

fn property_at(class: &Class, slot: PropertySlot) -> Option<&PropertyDefault> {
    match slot {
        PropertySlot::Instance(slot) => class.defaults.get(slot),
        PropertySlot::Static(slot) => class.static_properties.get(slot),
    }
}

fn missing_property(member: &MemberInfo) -> NativeError {
    exception(format!(
        "Property {}::${} does not exist",
        member.class, member.name
    ))
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
