//! `ReflectionProperty::setValue()` writes through the declared key even when it is private.

use std::rc::Rc;

use crate::lazy_continuation::{ReflectInitialize, ReflectedOperation};
use crate::lazy_objects::resolved_lazy_instance;
use phera_extension_api::reflection::{MemberInfo, PropertyRead, PropertySlot, PropertyWrite};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{LazyObject, Object, Value};

use crate::{
    Vm,
    classes::{Class, class::PropertyDefault},
};

use super::{exception, hook_calls::ReflectedHookCall};

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
                let object = required_object(object)?;
                let (property, slot) = if same_class(&class, &object) {
                    (property, instance_slot(slot))
                } else {
                    if !object.class.is_instance_of(&property.declaring) {
                        return Err(exception(
                            "Given object is not an instance of the class this property was declared in",
                        ));
                    }
                    Self::effective_property(property, slot, &object)
                };
                let object = resolved_lazy_instance(object.clone());
                if let Some(linked) = &property.hooks {
                    let hooks = &linked.hooks;
                    if matches!(mode, PropertyRead::Value)
                        && let Some(getter) = &hooks.get
                    {
                        return Err(ReflectedHookCall::Property {
                            function: getter.function,
                            object,
                            value: None,
                        }
                        .suspend());
                    }
                    if hooks.is_virtual {
                        if matches!(mode, PropertyRead::Initialized) {
                            return Ok(Value::Bool(true));
                        }
                        return Err(crate::classes::property_hooks::property_error(
                            &object,
                            &property.name,
                            if hooks.get.is_some() {
                                "Must not read from virtual property "
                            } else {
                                "Property "
                            },
                            if hooks.get.is_some() {
                                ""
                            } else {
                                " is write-only"
                            },
                        ));
                    }
                }
                if is_lazy_slot(&object, property) {
                    return Err(ReflectInitialize {
                        object,
                        operation: ReflectedOperation::Read(member.clone(), mode),
                    }
                    .suspend());
                }
                return self.reflected_backing_value(
                    property,
                    crate::property_storage::read_at(&object, &property.key, slot),
                    mode,
                );
            }
        };
        self.reflected_backing_value(property, value, mode)
    }

    fn reflected_backing_value(
        &self,
        property: &PropertyDefault,
        value: Option<Value>,
        mode: PropertyRead,
    ) -> NativeResult {
        if matches!(mode, PropertyRead::Initialized) {
            return Ok(Value::Bool(value.is_some()));
        }
        value.map(value_without_reference).map_or_else(
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
        mode: PropertyWrite,
    ) -> NativeResult {
        let (class, slot) = self.reflected_property_slot(member)?;
        let property = property_at(&class, slot).ok_or_else(|| missing_property(member))?;
        let (property, slot) = match object.as_ref() {
            Some(object) if property.static_storage.is_none() && !same_class(&class, object) => {
                if !object.class.is_instance_of(&member.class) {
                    return phera_extension_api::reflection::ReflectionContext::reflect_dynamic_property_write(
                        self, object.clone(), &member.name, value,
                    );
                }
                Self::effective_property(property, slot, object)
            }
            _ => (property, instance_slot(slot)),
        };
        let object = object.as_ref().cloned().map(resolved_lazy_instance);
        if let (Some(object), Some(linked)) = (&object, &property.hooks) {
            let hooks = &linked.hooks;
            if matches!(mode, PropertyWrite::Value)
                && let Some(setter) = &hooks.set
            {
                return Err(ReflectedHookCall::Property {
                    function: setter.function,
                    object: object.clone(),
                    value: Some(value),
                }
                .suspend());
            }
            if hooks.is_virtual {
                return Err(crate::classes::property_hooks::property_error(
                    object,
                    &property.name,
                    if hooks.set.is_some() {
                        "Must not write to virtual property "
                    } else {
                        "Property "
                    },
                    if hooks.set.is_some() {
                        ""
                    } else {
                        " is read-only"
                    },
                ));
            }
        }
        if !matches!(mode, PropertyWrite::RawWithoutInitialization)
            && let Some(object) = &object
            && is_lazy_slot(object, property)
        {
            return Err(ReflectInitialize {
                object: object.clone(),
                operation: ReflectedOperation::Write(member.clone(), value, mode),
            }
            .suspend());
        }
        let value = value_without_reference(value);
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
            let object = required_object(object.as_ref())?;
            if property.is_readonly && object.has(&property.key) {
                return Err(NativeError::error(format!(
                    "Cannot modify readonly property {}::${}",
                    property.declaring, property.name
                )));
            }
            if crate::property_storage::write_direct_at(object, &property.key, slot, &value, || {
                true
            }) {
                return Ok(Value::Null);
            }
            if let Some(Value::Reference(reference)) =
                crate::property_storage::read_at(object, &property.key, slot)
            {
                let converted = self.check_reference_assignment(&reference, value)?;
                *reference
                    .try_borrow_mut()
                    .map_err(|_| exception("Property value is already in use"))? = converted;
            } else {
                crate::property_storage::write_at(object, property.key.clone(), slot, value);
            }
        }
        Ok(Value::Null)
    }

    pub(super) fn effective_property<'a>(
        property: &'a PropertyDefault,
        slot: PropertySlot,
        object: &'a Object,
    ) -> (&'a PropertyDefault, Option<usize>) {
        if property.visibility != phera_bytecode::Visibility::Private
            && let Some(actual) = object.class.as_any().downcast_ref::<Class>()
            && let Some((index, actual_property)) =
                actual.defaults.iter().enumerate().find(|(_, candidate)| {
                    candidate.name == property.name
                        && candidate.visibility != phera_bytecode::Visibility::Private
                })
        {
            return (actual_property, Some(index));
        }
        (property, instance_slot(slot))
    }

    pub(super) fn reflected_property_slot(
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

pub(super) fn property_at(class: &Class, slot: PropertySlot) -> Option<&PropertyDefault> {
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

fn required_object<T>(object: Option<T>) -> Result<T, NativeError> {
    object.ok_or_else(|| NativeError::type_error(
        "ReflectionProperty::getValue(): Argument #1 ($object) must be provided for instance properties"
    ))
}

fn same_class(class: &Rc<Class>, object: &Object) -> bool {
    std::ptr::addr_eq(Rc::as_ptr(class), Rc::as_ptr(&object.class))
}

fn instance_slot(slot: PropertySlot) -> Option<usize> {
    match slot {
        PropertySlot::Instance(slot) => Some(slot),
        PropertySlot::Static(_) => None,
    }
}

fn value_without_reference(value: Value) -> Value {
    match value {
        Value::Reference(_) => value.deref().clone(),
        value => value,
    }
}

fn is_lazy_slot(object: &Object, property: &PropertyDefault) -> bool {
    matches!(object.lazy().as_deref(), Some(LazyObject::Pending { properties, .. })
        if properties.contains(&property.key))
}
