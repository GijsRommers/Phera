//! Property writes enforce declaration types and readonly state before modifying storage.

use crate::{Vm, access_mode::Mutation, inline_stack::InlineStack, path_update::PathKey};
use phera_extension_api::NativeError;
use phera_value::Value;

impl Vm {
    /// Class types use inheritance membership; scalar types reuse PHP's existing coercion policy.
    fn coerce_property(
        &mut self,
        value: Value,
        property: &crate::classes::class::PropertyDefault,
        declaration_type: &phera_bytecode::Type,
    ) -> Result<Value, NativeError> {
        if let Value::Object(object) = &*value.deref() {
            let matches = declaration_type
                .display
                .trim_start_matches('?')
                .split('|')
                .any(|name| {
                    let name = if name == "self" {
                        property.declaring.as_str()
                    } else {
                        name
                    };
                    self.classes
                        .get(object.class.name())
                        .is_some_and(|class| class.is_a(name))
                });
            if matches {
                return Ok(value.clone());
            }
        }
        let strict = self
            .current()
            .is_some_and(|frame| frame.function().strict_types);
        let actual = value.type_name();
        crate::types::coerce(value.clone(), declaration_type, strict.into(), self).map_err(|_| {
            NativeError::type_error(format!(
                "Cannot assign {actual} to property {}::${} of type {}",
                property.declaring, property.name, declaration_type.display
            ))
        })
    }

    /// Static property assignments enforce their declaration before replacing the shared cell.
    pub(crate) fn validate_static_mutation(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
        value: &mut Value,
    ) -> Result<(), NativeError> {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| internal_error!("static class"))?;
        let property = class
            .static_properties
            .iter()
            .find(|property| property.name == name)
            .ok_or_else(|| internal_error!("static property"))?;
        if let Some(declaration_type) = &property.ty {
            let converted = self.coerce_property(value.clone(), property, declaration_type)?;
            if !matches!(value, Value::Reference(_)) {
                *value = converted;
            }
        }
        property.value_initialized.set(true);
        Ok(())
    }

    /// Validate each object boundary, including indirect mutations of readonly arrays.
    pub(crate) fn validate_property_mutation(
        &mut self,
        mut current: Value,
        keys: &InlineStack<PathKey>,
        value: &mut Value,
        mutation: Mutation,
    ) -> Result<(), NativeError> {
        let mut steps = keys.iter().peekable();
        while let Some(key) = steps.next() {
            match (&current, key) {
                (Value::Object(object), PathKey::Property(key)) => {
                    let class = self
                        .classes
                        .get(object.class.name())
                        .ok_or_else(|| internal_error!("property class"))?;
                    let property = class.defaults.iter().find(|property| property.key == *key);
                    let name = crate::path_update::property_name(key);
                    if let Some(property) = property {
                        if property.is_readonly {
                            let scope = self
                                .current()
                                .and_then(|frame| frame.method.as_ref())
                                .map(|context| &context.scope);
                            if crate::property_storage::read(object, key).is_some()
                                || steps.peek().is_some()
                                || mutation == Mutation::Unset
                            {
                                return Err(NativeError::error(format!(
                                    "Cannot modify readonly property {}::${name}",
                                    property.declaring
                                )));
                            }
                            if scope.is_none_or(|scope| !scope.is_a(&property.declaring)) {
                                return Err(NativeError::error(format!(
                                    "Cannot modify protected(set) readonly property {}::${name} from global scope",
                                    property.declaring
                                )));
                            }
                        }
                        if steps.peek().is_none()
                            && mutation != Mutation::Unset
                            && let Some(declaration_type) = &property.ty
                        {
                            let converted =
                                self.coerce_property(value.clone(), property, declaration_type)?;
                            if !matches!(value, Value::Reference(_)) {
                                *value = converted;
                            }
                        }
                    } else if !crate::property_storage::read(object, key).is_some()
                        && mutation != Mutation::Unset
                    {
                        if class.is_readonly {
                            return Err(NativeError::error(format!(
                                "Cannot create dynamic property {}::${name}",
                                class.name
                            )));
                        }
                        if !class.allows_dynamic_properties {
                            self.diagnostic(
                                "Deprecated",
                                format!(
                                    "Creation of dynamic property {}::${name} is deprecated",
                                    class.name
                                ),
                            );
                        }
                    }
                    current = crate::property_storage::read(object, key)
                        .unwrap_or_default()
                        .deref()
                        .clone();
                }
                (Value::Array(array), PathKey::Index(Some(key))) => {
                    let next = array.get(key).unwrap_or(&Value::Null).deref().clone();
                    current = next;
                }
                _ => current = Value::Null,
            }
        }
        Ok(())
    }
}
