//! Property writes enforce declaration types and readonly state before modifying storage.

use crate::property_set_visibility::PropertyChange;
use crate::{Vm, access_mode::Mutation, inline_stack::InlineStack, path_update::PathKey};
use phera_extension_api::{NativeError, NativePropertyMutation};
use phera_value::Value;

impl Vm {
    /// An indexed write that promotes an empty static slot initializes it on success.
    pub(crate) fn prepare_static_array_promotion(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
    ) -> Result<std::rc::Rc<crate::reference_holders::StaticProperty>, NativeError> {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| internal_error!("static class"))?;
        let property = class
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == name)
            .ok_or_else(|| internal_error!("static property"))?;
        self.check_set_visibility(property, PropertyChange::Indirect)?;
        crate::path_dimensions::check_property_array_promotion(property)?;
        self.class_states
            .static_property(property)
            .ok_or_else(|| internal_error!("static storage"))
    }

    /// Class types use inheritance membership; scalar types reuse PHP's existing coercion policy.
    pub(crate) fn coerce_property(
        &mut self,
        value: Value,
        property: &crate::classes::class::PropertyDefault,
        declaration_type: &phera_bytecode::Type,
    ) -> Result<Value, NativeError> {
        self.coerce_property_in_mode(value, property, declaration_type, None)
    }

    /// An explicit mode overrides the caller’s strictness for internal property writes.
    pub(crate) fn coerce_property_in_mode(
        &mut self,
        value: Value,
        property: &crate::classes::class::PropertyDefault,
        declaration_type: &phera_bytecode::Type,
        strict: Option<bool>,
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
        let strict = strict.unwrap_or_else(|| {
            self.current()
                .is_some_and(|frame| frame.function().strict_types)
        });
        crate::types::coerce(value.clone(), declaration_type, strict.into(), self).map_err(|_| {
            let actual = crate::types::property_value_type(&value);
            NativeError::type_error(format!(
                "Cannot assign {actual} to property {}::${} of type {}",
                phera_value::class_display_name(&property.declaring),
                property.name,
                crate::types::php_type_display(declaration_type)
            ))
        })
    }

    /// Static property assignments enforce their declaration before replacing the shared cell.
    pub(crate) fn validate_static_mutation(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
        value: &mut Value,
        change: PropertyChange,
    ) -> Result<(), NativeError> {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| internal_error!("static class"))?;
        let property = class
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == name)
            .ok_or_else(|| internal_error!("static property"))?;
        self.check_set_visibility(property, change)?;
        if !matches!(change, PropertyChange::Direct | PropertyChange::Bind) {
            return Ok(());
        }
        if let Some(declaration_type) = &property.ty {
            match value {
                Value::Reference(reference) if matches!(change, PropertyChange::Bind) => {
                    self.coerce_property_binding(reference, property, declaration_type)?;
                }
                _ => {
                    let converted =
                        self.coerce_property(value.clone(), property, declaration_type)?;
                    if !matches!(value, Value::Reference(_)) {
                        *value = converted;
                    }
                }
            }
        }
        if let Some(storage) = self.class_states.static_property(property) {
            storage.value_initialized.set(true);
        }
        Ok(())
    }

    /// Reject readonly writes before the value's conversion can run PHP code.
    pub(crate) fn check_readonly_property_write(
        &self,
        object: &phera_value::Object,
        property: &crate::classes::class::PropertyDefault,
        indirect: bool,
        mutation: Mutation,
    ) -> Result<(), NativeError> {
        let name = &property.name;
        let key = &property.key;
        let declaring = phera_value::class_display_name(&property.declaring);
        if indirect || mutation == Mutation::Bind {
            return Err(NativeError::error(format!(
                "Cannot indirectly modify readonly property {declaring}::${name}"
            )));
        }
        if crate::property_storage::read(object, key).is_some()
            && !self.clone_can_update(object, key)
        {
            let action = if mutation == Mutation::Unset {
                "unset"
            } else {
                "modify"
            };
            return Err(NativeError::error(format!(
                "Cannot {action} readonly property {declaring}::${name}"
            )));
        }
        let change = if mutation == Mutation::Unset {
            PropertyChange::Unset
        } else {
            PropertyChange::Direct
        };
        self.check_set_visibility(property, change)
    }

    /// Validate each object boundary, including indirect mutations of readonly arrays.
    pub(crate) fn validate_property_mutation(
        &mut self,
        mut current: Value,
        keys: &InlineStack<PathKey>,
        value: &mut Value,
        mutation: Mutation,
    ) -> Result<bool, NativeError> {
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
                    if let Some(access) = class.property_access {
                        let change = if steps.peek().is_some() {
                            NativePropertyMutation::Indirect
                        } else {
                            match mutation {
                                Mutation::Assign => NativePropertyMutation::Assign(value),
                                Mutation::Bind => NativePropertyMutation::Bind,
                                Mutation::Unset => NativePropertyMutation::Unset,
                            }
                        };
                        if (access.mutate)(self, object, &name, change)? {
                            return Ok(true);
                        }
                        // A virtual name can bypass visibility only while its handler owns access.
                        self.property_key(object, &name)?;
                    }
                    self.notice_static_instance_access(class, &name);
                    if let Some(property) = property {
                        if property.is_readonly
                            && !(steps.peek().is_some()
                                && crate::property_storage::read(object, key)
                                    .is_some_and(|value| value.deref().is_object()))
                        {
                            self.check_readonly_property_write(
                                object,
                                property,
                                steps.peek().is_some(),
                                mutation,
                            )?;
                        }
                        if !property.is_readonly && property.set_visibility.is_some() {
                            let indirect = steps.peek().is_some();
                            if !indirect
                                || crate::property_storage::read(object, key)
                                    .map_or(mutation != Mutation::Unset, |value| {
                                        !value.deref().is_object()
                                    })
                            {
                                let change = match mutation {
                                    Mutation::Unset if !indirect => PropertyChange::Unset,
                                    Mutation::Assign if !indirect => PropertyChange::Direct,
                                    _ => PropertyChange::Indirect,
                                };
                                self.check_set_visibility(property, change)?;
                            }
                        }
                        if steps.peek().is_none()
                            && mutation != Mutation::Unset
                            && let Some(declaration_type) = &property.ty
                        {
                            match value {
                                Value::Reference(reference) if mutation == Mutation::Bind => {
                                    crate::reference_holders::ReferenceHolders::check_object_storage(
                                        object, key,
                                    )?;
                                    self.coerce_property_binding(
                                        reference,
                                        property,
                                        declaration_type,
                                    )?;
                                }
                                _ => {
                                    let converted = self.coerce_property(
                                        value.clone(),
                                        property,
                                        declaration_type,
                                    )?;
                                    if !matches!(value, Value::Reference(_)) {
                                        *value = converted;
                                    }
                                }
                            }
                        }
                        if steps.peek().is_none() {
                            if property.is_readonly && mutation != Mutation::Unset {
                                self.clone_updated(object, key);
                            }
                            if mutation == Mutation::Unset
                                && property.ty.is_some()
                                && class
                                    .magic
                                    .as_ref()
                                    .is_some_and(|magic| magic.has_properties())
                            {
                                self.mark_typed_property_unset(object, key.clone());
                            }
                        }
                    } else if !crate::property_storage::read(object, key).is_some()
                        && mutation != Mutation::Unset
                    {
                        if class.is_readonly
                            || class.kind == phera_bytecode::ClassKind::Enum
                            || class.origin == crate::classes::class::ClassOrigin::Closure
                            || crate::generators::generator_object(object)
                        {
                            return Err(NativeError::error(format!(
                                "Cannot create dynamic property {}::${name}",
                                phera_value::class_display_name(&class.name)
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
                    if mutation == Mutation::Bind && steps.peek().is_none() {
                        crate::reference_holders::ReferenceHolders::check_object_storage(
                            object, key,
                        )?;
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
        Ok(false)
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
            .and_then(|frame| frame.scope())
            .map(|scope| scope.name.as_str());
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
