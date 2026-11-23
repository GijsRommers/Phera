//! Instance property lookup applies lexical visibility before touching object storage.

use crate::{Vm, access_mode::Missing};
use phera_bytecode::Visibility;
use phera_extension_api::{NativeError, NativePropertyRead, NativeResult};
use phera_value::{ArrayKey, Value};

impl Vm {
    /// A property default must match its declared type even in a weak calling file.
    pub(crate) fn evaluate_property_initializer(
        &mut self,
        initializer: &crate::code_table::FunctionIndex,
        property: &crate::classes::class::PropertyDefault,
    ) -> NativeResult {
        let value = self.evaluate_initializer(initializer, &property.declaring)?;
        let Some(ty) = &property.ty else {
            return Ok(value);
        };
        let actual = value.type_name();
        crate::types::coerce(value, ty, crate::types::TypeChecking::Strict, self).map_err(|_| {
            NativeError::type_error(format!(
                "Cannot assign {actual} to property {}::${} of type {}",
                phera_value::class_display_name(&property.declaring),
                property.name,
                crate::types::php_type_display(ty),
            ))
        })
    }
}

impl Vm {
    /// `Post::$count`, `self::$count`, `static::$instances`: the class by
    /// its symbol's name key, or from the running frame for `self`,
    /// `static` and `parent`. A property visible from the frame whose
    /// storage this request made already is found through borrows alone;
    /// otherwise the class and name go on to the general path, which does
    /// not look the class up again. A class not declared yet, or a
    /// relative name without a class, takes the string path, which
    /// autoloads it or raises PHP's error.
    #[inline]
    fn named_static(
        &self,
        symbol: phera_bytecode::SymbolId,
        property: phera_bytecode::NameId,
    ) -> NamedStatic {
        let Some(function) = self.current().map(|frame| frame.frame.function) else {
            return NamedStatic::NotDeclared;
        };
        let class = match self.relative_class(symbol) {
            Some(Some(class)) => class,
            Some(None) => return NamedStatic::NotDeclared,
            None => match self
                .classes
                .by_key(self.code.unit_symbol_key(function, symbol))
            {
                Some(class) => class.clone(),
                None => return NamedStatic::NotDeclared,
            },
        };
        let name = &self.code.unit(function).names[property.index()];
        if class.kind != phera_bytecode::ClassKind::Trait
            && let Some(property) = class
                .static_properties
                .iter()
                .rev()
                .find(|candidate| candidate.name == *name)
            && self.static_visible(property)
            && let Some(storage) = self.class_states.existing_static(property)
            && storage.initialized.get()
        {
            return NamedStatic::Ready(storage.clone());
        }
        NamedStatic::Found(class, name.clone())
    }

    /// Whether the running code may access the static property: public,
    /// or private or protected from a class scope PHP allows.
    pub(crate) fn static_visible(&self, property: &crate::classes::class::PropertyDefault) -> bool {
        let scope = self.current().and_then(|frame| frame.scope());
        match property.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                scope.is_a(&property.declaring)
                    || self
                        .classes
                        .get(&property.declaring)
                        .is_some_and(|declaring| declaring.is_a(&scope.name))
            }),
        }
    }

    /// The compiled file of the running frame.
    fn running_unit(&self) -> Result<std::sync::Arc<phera_bytecode::CompiledUnit>, NativeError> {
        self.current()
            .map(|frame| frame.unit().clone())
            .ok_or_else(|| internal_error!("static property frame"))
    }

    /// Dynamic static targets accept either an object handle or a class-name string.
    pub(crate) fn static_property_names(
        &mut self,
        class: crate::static_places::StaticClass,
        name: crate::static_places::StaticName,
    ) -> Result<(String, String), NativeError> {
        use crate::static_places::{StaticClass, StaticName};
        let class = match class {
            StaticClass::Named(symbol) => match self.relative_class(symbol) {
                Some(Some(class)) => class.name.clone(),
                // A class by its name, or PHP's error for a relative name
                // without a class.
                _ => self.class_symbol(symbol)?,
            },
            StaticClass::Register(class) => match self.read_local(class) {
                Value::Object(object) => object.class.name().to_owned(),
                Value::InternedStr(name) => name.to_string_lossy(),
                Value::String(name) => name.to_string_lossy(),
                _ => {
                    return Err(NativeError::error(
                        "Class name must be a valid object or a string",
                    ));
                }
            },
        };
        let name = match name {
            StaticName::Named(name) => self.running_unit()?.names[name.index()].clone(),
            StaticName::Register(name) => {
                let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                phera_value::to_php_string(&diagnostics.vm.operand(name), &mut diagnostics.buffer)?
                    .to_string_lossy()
            }
        };
        Ok((class, name))
    }

    /// Inherited static declarations share reference cells until a child redeclares them.
    pub(crate) fn static_property(
        &mut self,
        class: crate::static_places::StaticClass,
        name: crate::static_places::StaticName,
    ) -> NativeResult {
        let storage = self.static_storage(class, name)?;
        Ok(Value::Reference(storage.reference.borrow().clone()))
    }

    /// The storage of a static property the running code may access, its
    /// default evaluated: PHP's error when there is no such property or the
    /// code may not access it.
    pub(crate) fn static_storage(
        &mut self,
        class: crate::static_places::StaticClass,
        name: crate::static_places::StaticName,
    ) -> Result<std::rc::Rc<crate::reference_holders::StaticProperty>, NativeError> {
        use crate::static_places::{StaticClass, StaticName};
        let found = match (class, name) {
            (StaticClass::Named(symbol), StaticName::Named(property)) => {
                match self.named_static(symbol, property) {
                    NamedStatic::Ready(storage) => return Ok(storage),
                    NamedStatic::Found(class, name) => Some((class, name)),
                    NamedStatic::NotDeclared => None,
                }
            }
            _ => None,
        };
        let (class, name) = match found {
            Some(found) => found,
            None => {
                let (class_name, name) = self.static_property_names(class, name)?;
                let class = self
                    .classes
                    .get(&class_name)
                    .ok_or_else(|| Self::class_miss(&class_name, "Class"))?;
                (class, name)
            }
        };
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
        if !self.static_visible(property) {
            let visibility = if property.visibility == Visibility::Private {
                "private"
            } else {
                "protected"
            };
            // PHP names the class the access went through (`Child::$secret`
            // for `self::$secret` in a child of the declaring class).
            return Err(NativeError::error(format!(
                "Cannot access {visibility} property {}::${name}",
                phera_value::class_display_name(&class.name)
            )));
        }
        if class.kind == phera_bytecode::ClassKind::Trait {
            self.diagnostic(
                "Deprecated",
                format!(
                    "Accessing static trait property {}::${name} is deprecated, it should only be accessed on a class using the trait",
                    class.name,
                ),
            );
        }
        let storage = self
            .class_states
            .static_property(property)
            .ok_or_else(|| internal_error!("static storage"))?;
        if !storage.initialized.get() {
            if let Some(initializer) = &property.initializer {
                let value = self.evaluate_property_initializer(initializer, property)?;
                *storage.reference.borrow().borrow_mut() = value;
            }
            storage.initialized.set(true);
        }
        Ok(storage)
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
        if let Some(value) = self.native_property_read(&object, &name, missing)? {
            return Ok(value);
        }
        self.read_declared_property(&object, &name, missing)
    }

    pub(crate) fn native_property_read(
        &mut self,
        object: &phera_value::Object,
        name: &str,
        missing: Missing,
    ) -> Result<Option<Value>, NativeError> {
        let Some(access) = native_property_access(object) else {
            return Ok(None);
        };
        let mode = match missing {
            Missing::Warn => NativePropertyRead::Read,
            Missing::Silent => NativePropertyRead::Quiet,
            Missing::Isset => NativePropertyRead::Isset,
        };
        (access.read)(self, object, name, mode)
    }

    pub(crate) fn read_declared_property(
        &mut self,
        object: &phera_value::Object,
        name: &str,
        missing: Missing,
    ) -> NativeResult {
        let key = match self.property_key(object, name) {
            Ok(key) => key,
            Err(_) if missing != Missing::Warn => return Ok(Value::Null),
            Err(error) => return Err(error),
        };
        if missing == Missing::Warn
            && let Some(class) = object
                .class
                .as_any()
                .downcast_ref::<crate::classes::Class>()
        {
            self.notice_static_instance_access(class, name);
        }
        if let Some(value) = crate::property_storage::read(object, &key) {
            return Ok(value.deref().clone());
        }
        if missing == Missing::Warn
            && let Some(class) = object
                .class
                .as_any()
                .downcast_ref::<crate::classes::Class>()
            && let Some(property) = class
                .defaults
                .iter()
                .find(|property| property.key == key && property.ty.is_some())
        {
            return Err(NativeError::error(format!(
                "Typed property {}::${name} must not be accessed before initialization",
                phera_value::class_display_name(&property.declaring)
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
        current: Value,
    ) -> Result<crate::inline_stack::InlineStack<crate::path_update::PathKey>, NativeError> {
        self.property_keys_for(place.steps()?, current)
    }

    pub(crate) fn property_keys_for(
        &mut self,
        steps: impl Iterator<Item = phera_bytecode::Step>,
        mut current: Value,
    ) -> Result<crate::inline_stack::InlineStack<crate::path_update::PathKey>, NativeError> {
        use crate::path_update::PathKey;
        let mut keys = crate::inline_stack::InlineStack::new();
        for step in steps {
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
                    let resolved = match self.property_key(object, &name) {
                        Ok(resolved) => resolved,
                        Err(error) => {
                            let Some(access) = native_property_access(object) else {
                                return Err(error);
                            };
                            if (access.read)(self, object, &name, NativePropertyRead::Exists)?
                                .is_none()
                            {
                                return Err(error);
                            }
                            key
                        }
                    };
                    PathKey::Property(resolved)
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
        self.resolve_property_key(object, name).map(|(key, _)| key)
    }

    /// The declaration position is a slot hint. Object storage validates the
    /// key there because redeclarations can share a storage slot.
    pub(crate) fn resolve_property_key(
        &self,
        object: &phera_value::Object,
        name: &str,
    ) -> Result<(ArrayKey, Option<usize>), NativeError> {
        if name.starts_with('\0') {
            return Err(NativeError::error(
                "Cannot access property starting with \"\\0\"",
            ));
        }
        let class = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
            .ok_or_else(|| internal_error!("property class"))?;
        let scope = self.current().and_then(|frame| frame.scope());
        let instance = class.instance_property_named(name, scope.map(|scope| scope.name.as_str()));
        let property = instance.map(|i| &class.defaults[i]).or_else(|| {
            class
                .static_properties
                .iter()
                .rev()
                .find(|property| property.name == name)
        });
        let property = property.filter(|property| {
            property.visibility != Visibility::Private
                || property.declaring == class.name
                || scope.is_some_and(|scope| scope.name == property.declaring)
        });
        let Some(property) = property else {
            return Ok((ArrayKey::str(name), None));
        };
        let allowed = match property.visibility {
            Visibility::Public => true,
            Visibility::Private => scope.is_some_and(|scope| scope.name == property.declaring),
            Visibility::Protected => scope.is_some_and(|scope| {
                let owner = property
                    .hooks
                    .as_ref()
                    .map_or(property.declaring.as_str(), |hooks| &hooks.prototype);
                scope.is_a(owner)
                    || self
                        .classes
                        .get(owner)
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
        if instance.is_some() {
            Ok((property.key.clone(), instance))
        } else {
            Ok((ArrayKey::str(name), None))
        }
    }
}

pub(crate) fn native_property_access(
    object: &phera_value::Object,
) -> Option<&'static phera_extension_api::NativePropertyAccess> {
    object
        .class
        .as_any()
        .downcast_ref::<crate::classes::Class>()?
        .property_access
}

/// What a static property of a named class was found to be.
enum NamedStatic {
    /// Its storage, found without the general path.
    Ready(std::rc::Rc<crate::reference_holders::StaticProperty>),
    /// Its class and name, for the general path.
    Found(std::rc::Rc<crate::classes::Class>, String),
    /// The class is not declared yet: the string path autoloads it.
    NotDeclared,
}
