//! A serialized `"x"` can restore a declared protected `$x` without running `__set`.

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, PropertyPurpose};
use phera_value::{ArrayKey, Object, PhpArray, PropertyScope, Value, split_property_key};

use super::property_index::DeclaredProperties;
use crate::Vm;
use crate::classes::class::PropertyDefault;

impl Vm {
    pub(crate) fn ordered_serialization_properties(
        &mut self,
        object: &Object,
    ) -> Result<PhpArray, NativeError> {
        let properties = self.object_properties(object, PropertyPurpose::Serialize)?;
        let Some(class) = self.classes.for_object(object) else {
            return Ok(properties);
        };
        if class.defaults.is_empty() {
            return Ok(properties);
        }
        let layout = class
            .defaults
            .iter()
            .filter(|property| properties.contains_key(&property.key));
        if layout
            .zip(properties.keys())
            .all(|(property, key)| property.key == key)
        {
            return Ok(properties);
        }
        let mut ordered = PhpArray::with_capacity(properties.len());
        for property in &class.defaults {
            if let Some(value) = properties.get(&property.key) {
                ordered.insert(property.key.clone(), value.clone());
            }
        }
        for (key, value) in properties.iter() {
            if !ordered.contains_key(&key) {
                ordered.insert(key, value.clone());
            }
        }
        Ok(ordered)
    }

    pub(crate) fn resolve_serialized_property(
        &mut self,
        object: &Object,
        key: ArrayKey,
    ) -> Option<ArrayKey> {
        let class = self.classes.for_object(object)?;
        let ArrayKey::String(spelling) = &key else {
            return Some(key);
        };
        let declarations = DeclaredProperties::new(self, &class);
        if class.defaults.is_empty() {
            return Some(key);
        }
        if let Some(property) = declarations.key(&key) {
            return self.unserialized_storage_key(property);
        }
        let (name, scope) = split_property_key(spelling.as_bytes());
        if spelling.as_bytes().first() == Some(&0)
            && (scope == PropertyScope::Public || name.is_empty())
        {
            let message = if spelling.as_bytes().get(1) == Some(&0) {
                "Illegal member variable name"
            } else {
                "Corrupt member variable name"
            };
            self.diagnostic("Notice", message.into());
            return None;
        }
        if let PropertyScope::Private(owner) = scope
            && !owner.eq_ignore_ascii_case(class.name.as_bytes())
        {
            return Some(key);
        }
        match declarations.name(name) {
            Some(property) => self.unserialized_storage_key(property),
            None => Some(key),
        }
    }

    fn unserialized_storage_key(&mut self, property: &PropertyDefault) -> Option<ArrayKey> {
        if property
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.hooks.is_virtual)
        {
            self.diagnostic(
                "Warning",
                format!(
                    "unserialize(): Cannot unserialize value for virtual property {}::${}",
                    phera_value::class_display_name(&property.declaring),
                    property.name,
                ),
            );
            return None;
        }
        Some(property.key.clone())
    }

    pub(crate) fn check_serialized_dynamic_property(
        &mut self,
        object: &Object,
        key: &ArrayKey,
    ) -> Result<(), NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("serialized property class"))?;
        if class.allows_dynamic_properties
            || class.name == phera_extension_api::serialization::INCOMPLETE_CLASS
            || DeclaredProperties::new(self, &class).key(key).is_some()
        {
            return Ok(());
        }
        let ArrayKey::String(key) = key else {
            return Ok(());
        };
        let (name, _) = split_property_key(key.as_bytes());
        let name = String::from_utf8_lossy(name);
        if class.is_readonly {
            return Err(NativeError::error(format!(
                "Cannot create dynamic property {}::${name}",
                class.name
            )));
        }
        self.diagnostic(
            "Deprecated",
            format!(
                "Creation of dynamic property {}::${name} is deprecated",
                class.name
            ),
        );
        Ok(())
    }

    pub(crate) fn write_unserialized_properties(
        &mut self,
        object: &Rc<Object>,
        properties: PhpArray,
    ) -> Result<(), NativeError> {
        let class = self
            .classes
            .for_object(object)
            .ok_or_else(|| internal_error!("serialized property class"))?;
        let declarations = DeclaredProperties::new(self, &class);
        for (key, value) in properties.iter() {
            let property = declarations.key(&key);
            let value = if let Some(property) = property
                && let Some(ty) = &property.ty
            {
                let converted =
                    self.coerce_property_in_mode(value.clone(), property, ty, Some(true))?;
                if let Value::Reference(cell) = value {
                    if !converted.is_reference() {
                        self.check_serialized_reference_widening(cell, &converted, property)?;
                        *cell
                            .try_borrow_mut()
                            .map_err(|_| internal_error!("unserialized property reference"))? =
                            converted;
                    }
                    value.clone()
                } else {
                    converted
                }
            } else {
                value.clone()
            };
            if let Value::Reference(cell) = &value {
                crate::reference_holders::ReferenceHolders::track_serialized_property(
                    &self.property_references,
                    object,
                    &key,
                    cell,
                    property,
                )?;
            }
            object.set(key, value);
        }
        let properties = self.ordered_serialization_properties(object)?;
        object.update(|storage| storage.replace_all(properties));
        Ok(())
    }

    fn check_serialized_reference_widening(
        &mut self,
        cell: &Rc<std::cell::RefCell<Value>>,
        value: &Value,
        property: &crate::classes::class::PropertyDefault,
    ) -> Result<(), NativeError> {
        let address = Rc::as_ptr(cell) as usize;
        let holders = self
            .property_references
            .borrow()
            .constraints
            .get(&address)
            .cloned();
        for (_, holder) in holders.into_iter().flatten() {
            if let Some(ty) = &holder.ty
                && self
                    .coerce_property_in_mode(value.clone(), &holder, ty, Some(true))
                    .is_err()
                && let Some(property_type) = &property.ty
            {
                let original = cell
                    .try_borrow()
                    .map_err(|_| internal_error!("unserialized property reference"))?;
                return Err(NativeError::type_error(format!(
                    "Reference with value of type {} held by property {}::${} of type {} \
                     is not compatible with property {}::${} of type {}",
                    original.type_name(),
                    holder.declaring,
                    holder.name,
                    crate::types::php_type_display(ty),
                    property.declaring,
                    property.name,
                    crate::types::php_type_display(property_type)
                )));
            }
        }
        Ok(())
    }
}
