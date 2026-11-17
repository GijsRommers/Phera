//! Instance property lookup applies lexical visibility before touching object storage.

use crate::{Vm, access_mode::Missing};
use phera_bytecode::Visibility;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value};

impl Vm {
    /// User-facing property enumeration strips visibility prefixes after checking the current scope.
    pub(crate) fn visible_properties(&self, object: &phera_value::Object) -> phera_value::PhpArray {
        let mut result = phera_value::PhpArray::new();
        for (key, value) in object.to_array().iter() {
            let name = crate::path_update::property_name(key);
            if self
                .property_key(object, &name)
                .is_ok_and(|resolved| resolved == *key)
            {
                result.insert(ArrayKey::str(name), value.deref().clone());
            }
        }
        result
    }

    /// Class variables expose defaults rather than values modified on existing objects.
    pub(crate) fn class_variables(&mut self, name: &str) -> NativeResult {
        let class = self
            .classes
            .get(name)
            .ok_or_else(|| NativeError::error(format!("Class \"{name}\" not found")))?;
        let temporary = self.classes.allocate(class.clone());
        let mut result = phera_value::PhpArray::new();
        for property in class.defaults.iter().chain(&class.static_properties) {
            if !property.initialized || self.property_key(&temporary, &property.name).is_err() {
                continue;
            }
            let value = if let Some(initializer) = &property.initializer {
                self.evaluate_initializer(initializer, &property.declaring)?
            } else {
                property.static_storage.as_ref().map_or_else(
                    || property.value.deref().clone(),
                    |storage| storage.reference.borrow().borrow().deref().clone(),
                )
            };
            result.insert(ArrayKey::str(property.name.as_bytes()), value);
        }
        Ok(Value::Array(std::rc::Rc::new(result)))
    }

    /// Dynamic static targets accept either an object handle or a class-name string.
    pub(crate) fn static_property_names(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
    ) -> Result<(String, String), NativeError> {
        let class = match self.read_local(class) {
            Value::Object(object) => object.class.name().to_owned(),
            Value::String(name) => name.to_string_lossy(),
            _ => {
                return Err(NativeError::error(
                    "Class name must be a valid object or a string",
                ));
            }
        };
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let name =
            phera_value::to_php_string(&diagnostics.vm.operand(name), &mut diagnostics.buffer)?
                .to_string_lossy();
        Ok((class, name))
    }

    /// Inherited static declarations share reference cells until a child redeclares them.
    pub(crate) fn static_property(
        &mut self,
        class: phera_bytecode::Reg,
        name: phera_bytecode::Reg,
    ) -> NativeResult {
        let (class_name, name) = self.static_property_names(class, name)?;
        let class = self
            .classes
            .get(&class_name)
            .ok_or_else(|| NativeError::error(format!("Class \"{class_name}\" not found")))?;
        let property = class
            .static_properties
            .iter()
            .rev()
            .find(|property| property.name == name)
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Access to undeclared static property {}::${name}",
                    class.name
                ))
            })?;
        let scope = self
            .current()
            .and_then(|frame| frame.method.as_ref())
            .map(|context| &context.scope);
        let allowed = match property.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| scope.is_a(&property.declaring)),
        };
        if !allowed {
            let visibility = if property.visibility == Visibility::Private {
                "private"
            } else {
                "protected"
            };
            return Err(NativeError::error(format!(
                "Cannot access {visibility} property {}::${name}",
                property.declaring
            )));
        }
        if !property.static_initialized.get() {
            if let Some(initializer) = &property.initializer {
                let value = self.evaluate_initializer(initializer, &property.declaring)?;
                if let Some(storage) = &property.static_storage {
                    *storage.reference.borrow().borrow_mut() = value;
                }
            }
            property.static_initialized.set(true);
        }
        let storage = property
            .static_storage
            .as_ref()
            .ok_or_else(|| internal_error!("static storage"))?;
        Ok(Value::Reference(storage.reference.borrow().clone()))
    }

    /// Read declared and dynamic properties with PHP's quiet-access behavior.
    pub(crate) fn read_property(
        &mut self,
        value: Value,
        key: ArrayKey,
        missing: Missing,
    ) -> NativeResult {
        let name = crate::path_update::property_name(&key);
        let Value::Object(object) = value else {
            if missing == Missing::Warn {
                self.warn(format!(
                    "Attempt to read property \"{name}\" on {}",
                    value.type_name()
                ));
            }
            return Ok(Value::Null);
        };
        let key = match self.property_key(&object, &name) {
            Ok(key) => key,
            Err(_) if missing == Missing::Silent => return Ok(Value::Null),
            Err(error) => return Err(error),
        };
        if missing == Missing::Warn
            && let Some(class) = object
                .class
                .as_any()
                .downcast_ref::<crate::classes::Class>()
        {
            self.notice_static_instance_access(class, &name);
        }
        if let Some(value) = crate::property_storage::read(&object, &key) {
            return Ok(value.deref().clone());
        }
        let declared = self.classes.get(object.class.name()).is_some_and(|class| {
            class
                .defaults
                .iter()
                .any(|property| property.key == key && property.ty.is_some())
        });
        if declared && missing == Missing::Warn {
            return Err(NativeError::error(format!(
                "Typed property {}::${name} must not be accessed before initialization",
                object.class.name()
            )));
        }
        if missing == Missing::Warn {
            self.warn(format!(
                "Undefined property: {}::${name}",
                object.class.name()
            ));
        }
        Ok(Value::Null)
    }

    /// Resolve visibility before detaching any values so failed writes preserve storage.
    pub(crate) fn property_keys(
        &mut self,
        place: &crate::AccessPath,
        mut current: Value,
    ) -> Result<crate::inline_stack::InlineStack<crate::path_update::PathKey>, NativeError> {
        use crate::path_update::PathKey;
        let mut keys = crate::inline_stack::InlineStack::new();
        for step in place.steps()? {
            let key = match step {
                phera_bytecode::Step::Index(register) => {
                    let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                    PathKey::Index(Some(ArrayKey::from_value_diag(
                        &diagnostics.vm.operand(register),
                        &mut diagnostics.buffer,
                    )?))
                }
                phera_bytecode::Step::Append => PathKey::Index(None),
                phera_bytecode::Step::Property(name) => {
                    PathKey::Property(ArrayKey::str(self.name(name)?))
                }
                phera_bytecode::Step::DynamicProperty(register) => {
                    let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                    PathKey::Property(ArrayKey::str(
                        phera_value::to_php_string(
                            &diagnostics.vm.operand(register),
                            &mut diagnostics.buffer,
                        )?
                        .as_bytes(),
                    ))
                }
            };
            let key = match (&current, key) {
                (Value::Object(object), PathKey::Property(key)) => {
                    let name = crate::path_update::property_name(&key);
                    PathKey::Property(self.property_key(object, &name)?)
                }
                (_, key) => key,
            };
            current = match (&current, &key) {
                (Value::Object(object), PathKey::Property(key)) => {
                    crate::property_storage::read(object, key)
                        .unwrap_or_default()
                        .deref()
                        .clone()
                }
                (Value::Array(array), PathKey::Index(Some(key))) => {
                    array.get(key).unwrap_or(&Value::Null).deref().clone()
                }
                _ => Value::Null,
            };
            keys.push(key);
        }
        Ok(keys)
    }

    /// Parent private slots coexist with child slots; lexical private lookup wins.
    pub(crate) fn property_key(
        &self,
        object: &phera_value::Object,
        name: &str,
    ) -> Result<ArrayKey, NativeError> {
        let class = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .ok_or_else(|| internal_error!("property class"))?;
        let scope = self
            .current()
            .and_then(|frame| frame.method.as_ref())
            .map(|context| &context.scope);
        let property = scope
            .and_then(|scope| {
                class.defaults.iter().find(|property| {
                    property.name == name
                        && property.visibility == Visibility::Private
                        && property.declaring == scope.name
                })
            })
            .or_else(|| {
                class
                    .defaults
                    .iter()
                    .rev()
                    .find(|property| property.name == name)
            })
            .or_else(|| {
                class
                    .static_properties
                    .iter()
                    .rev()
                    .find(|property| property.name == name)
            });
        let property = property.filter(|property| {
            !(property.visibility == Visibility::Private
                && scope.is_some_and(|scope| {
                    scope.name != property.declaring && scope.is_a(&property.declaring)
                }))
        });
        let Some(property) = property else {
            return Ok(ArrayKey::str(name));
        };
        let allowed = match property.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&property.declaring)
                    || self
                        .classes
                        .get(&property.declaring)
                        .is_some_and(|declaring| declaring.is_a(&scope.name))
            }),
        };
        if !allowed {
            let visibility = if property.visibility == Visibility::Private {
                "private"
            } else {
                "protected"
            };
            return Err(NativeError::error(format!(
                "Cannot access {visibility} property {}::${name}",
                property.declaring
            )));
        }
        if class
            .defaults
            .iter()
            .any(|candidate| std::ptr::eq(candidate, property))
        {
            Ok(property.key.clone())
        } else {
            Ok(ArrayKey::str(name))
        }
    }
}
