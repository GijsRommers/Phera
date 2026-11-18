//! A serialized `"x"` can restore a declared protected `$x` without running `__set`.

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, PropertyPurpose};
use phera_value::{ArrayKey, Object, PhpArray, PropertyScope, Value, split_property_key};

use crate::Vm;

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
        if class.defaults.is_empty() || class.defaults.iter().any(|prop| prop.key == key) {
            return Some(key);
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
        Some(
            class
                .defaults
                .iter()
                .rev()
                .find(|prop| {
                    prop.name.as_bytes() == name
                        && (prop.visibility != phera_bytecode::Visibility::Private
                            || prop.declaring == class.name)
                })
                .map_or(key, |prop| prop.key.clone()),
        )
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
        if class.defaults.iter().any(|prop| prop.key == *key)
            || class.allows_dynamic_properties
            || class.name == phera_extension_api::serialization::INCOMPLETE_CLASS
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
        for (key, value) in properties.iter() {
            let value = if let Some(property) = class.defaults.iter().find(|prop| prop.key == key)
                && let Some(ty) = &property.ty
            {
                let converted =
                    self.coerce_property_in_mode(value.clone(), property, ty, Some(true))?;
                if let Value::Reference(cell) = value {
                    if !converted.is_reference() {
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
            object.set(key, value);
        }
        let properties = self.ordered_serialization_properties(object)?;
        object.update(|storage| *storage = properties);
        self.track_object_property_references(object)
    }
}
