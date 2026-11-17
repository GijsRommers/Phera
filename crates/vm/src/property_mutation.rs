//! Property writes enforce declaration types and readonly state before modifying storage.

use crate::{Vm, access_mode::Mutation, inline_stack::InlineStack, path_update::PathKey};
use phera_extension_api::NativeError;
use phera_value::Value;

impl Vm {
    /// Class types use inheritance membership; scalar types reuse PHP's existing coercion policy.
    pub(crate) fn coerce_property(
        &mut self,
        value: Value,
        property: &crate::classes::class::PropertyDefault,
        declaration_type: &phera_bytecode::Type,
    ) -> Result<Value, NativeError> {
        if crate::types::accepts_as_is(&value, declaration_type) {
            return Ok(value);
        }
        if let Some(float) = crate::types::widen_int_to_float(&value, declaration_type.mask) {
            return Ok(float);
        }
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
                    let class = object
                        .class
                        .as_any()
                        .downcast_ref::<crate::classes::Class>()
                        .ok_or_else(|| internal_error!("property class"))?;
                    let property = class.defaults.iter().find(|property| property.key == *key);
                    let name = crate::path_update::property_name(key);
                    self.notice_static_instance_access(class, &name);
                    if let Some(property) = property {
                        if property.is_readonly
                            && !(steps.peek().is_some()
                                && crate::property_storage::read(object, key)
                                    .is_some_and(|value| value.deref().is_object()))
                        {
                            if mutation == Mutation::Bind {
                                return Err(NativeError::error(format!(
                                    "Cannot indirectly modify readonly property {}::${name}",
                                    property.declaring
                                )));
                            }
                            let scope = self
                                .current()
                                .and_then(|frame| frame.method())
                                .map(|context| &context.scope);
                            if crate::property_storage::read(object, key).is_some()
                                || steps.peek().is_some()
                            {
                                let action = if mutation == Mutation::Unset {
                                    "unset"
                                } else {
                                    "modify"
                                };
                                return Err(NativeError::error(format!(
                                    "Cannot {action} readonly property {}::${name}",
                                    property.declaring
                                )));
                            }
                            if scope.is_none_or(|scope| !scope.is_a(&property.declaring)) {
                                let action = if mutation == Mutation::Unset {
                                    "unset"
                                } else {
                                    "modify"
                                };
                                let from = scope
                                    .map(|scope| format!("scope {}", scope.name))
                                    .unwrap_or_else(|| "global scope".into());
                                return Err(NativeError::error(format!(
                                    "Cannot {action} protected(set) readonly property {}::${name} from {from}",
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
                        if class.is_readonly
                            || class.origin == crate::classes::class::ClassOrigin::Closure
                        {
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

impl Vm {
    /// Static storage is never read or modified through an instance property access.
    pub(crate) fn notice_static_instance_access(
        &mut self,
        class: &crate::classes::Class,
        name: &str,
    ) {
        if class.static_properties.is_empty() {
            return;
        }
        let scope = self
            .current()
            .and_then(|frame| frame.method())
            .map(|context| context.scope.name.as_str());
        if class.defaults.iter().any(|property| {
            property.name == name
                && (property.visibility != phera_bytecode::Visibility::Private
                    || scope == Some(property.declaring.as_str()))
        }) {
            return;
        }
        if class
            .static_properties
            .iter()
            .any(|property| property.name == name)
        {
            self.diagnostic(
                "Notice",
                format!(
                    "Accessing static property {}::${name} as non static",
                    class.name
                ),
            );
        }
    }
}
