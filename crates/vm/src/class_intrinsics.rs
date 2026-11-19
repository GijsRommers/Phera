//! `class_exists('A')` may load A, whereas `class_exists('A', false)` only checks the table.

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{PhpArray, Value};

use crate::{Vm, calls::callback_binding::ArgumentOrigin, function_introspection::deprecated};

pub(crate) use crate::class_function::ClassFunction;

impl Vm {
    pub(crate) async fn class_intrinsic(
        &mut self,
        function: ClassFunction,
        mut arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        use ClassFunction as Operation;
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(
                function.name(),
                function.parameters(),
                &mut arguments,
                vm,
            )
        })?;
        let value = arguments.first().cloned().unwrap_or_default();
        let option = |index: usize, default: bool| {
            arguments.get(index).map_or(default, phera_value::to_bool)
        };

        match function {
            Operation::DeclaredClasses
            | Operation::DeclaredInterfaces
            | Operation::DeclaredTraits => {
                return self.declared_classes(function);
            }
            Operation::GetCalledClass => {
                return self.called_class_name();
            }
            Operation::GetClass | Operation::GetParentClass if arguments.is_empty() => {
                if matches!(function, Operation::GetClass)
                    && self.current().and_then(|frame| frame.scope()).is_none()
                {
                    return Err(NativeError::error(
                        "get_class() without arguments must be called from within a class",
                    ));
                }
                let signature = format!("{}() without arguments", function.name());
                return Err(deprecated(&signature, "8.3"));
            }
            Operation::GetClass
            | Operation::GetObjectVars
            | Operation::GetMangledObjectVars
            | Operation::ObjectId
            | Operation::ObjectHash => {
                let Value::Object(object) = value else {
                    return Err(internal_error!("class function object"));
                };
                if matches!(function, Operation::GetObjectVars)
                    && let Some(class) = self.hooked_property_class(&object)
                {
                    let native = crate::error_traces::NativeTrace::new(
                        function.name(),
                        &[Value::Object(object.clone())],
                    );
                    let properties = self
                        .enumerate_hooked_native(
                            object,
                            class,
                            phera_extension_api::PropertyPurpose::ObjectVars,
                            native,
                        )
                        .await?;
                    return Ok(Value::from(properties));
                }
                return Ok(match function {
                    Operation::GetClass => Value::from(object.class.name()),
                    Operation::GetObjectVars => Value::from(self.visible_properties(
                        &object,
                        phera_extension_api::PropertyPurpose::ObjectVars,
                    )?),
                    Operation::GetMangledObjectVars => {
                        Value::from(phera_extension_api::Context::object_properties(
                            self,
                            &object,
                            phera_extension_api::PropertyPurpose::ArrayCast,
                        )?)
                    }
                    Operation::ObjectId => Value::Int(object.id.into()),
                    _ => Value::from(object.handle_hash()),
                });
            }
            Operation::ClassAlias => {
                return self
                    .class_alias(&string(&value), &string(&arguments[1]), option(2, true))
                    .await;
            }
            _ => {}
        }

        let autoload = match function {
            Operation::IsA | Operation::IsSubclassOf => {
                option(2, matches!(function, Operation::IsSubclassOf))
            }
            Operation::ClassExists
            | Operation::InterfaceExists
            | Operation::TraitExists
            | Operation::EnumExists
            | Operation::ClassParents
            | Operation::ClassImplements
            | Operation::ClassUses => option(1, true),
            _ => true,
        };
        let class = match &value {
            Value::Object(object) => self.classes.for_object(object),
            Value::String(name) => {
                let name = name.to_string_lossy();
                if autoload {
                    self.lookup_class_from(&name, Some((function.name(), &arguments)))
                        .await?
                } else if matches!(function, Operation::IsA | Operation::IsSubclassOf) {
                    None
                } else {
                    self.classes.get(&name)
                }
            }
            _ => None,
        };
        match function {
            Operation::ClassExists => Ok(Value::Bool(
                class.is_some_and(|class| class.kind.is_class()),
            )),
            Operation::InterfaceExists => Ok(Value::Bool(
                class.is_some_and(|class| class.kind == ClassKind::Interface),
            )),
            Operation::TraitExists => Ok(Value::Bool(
                class.is_some_and(|class| class.kind == ClassKind::Trait),
            )),
            Operation::EnumExists => Ok(Value::Bool(
                class.is_some_and(|class| class.kind == ClassKind::Enum),
            )),
            Operation::IsA | Operation::IsSubclassOf => {
                let target = self.classes.get(&string(&arguments[1]));
                Ok(Value::Bool(class.zip(target).is_some_and(
                    |(class, target)| {
                        class.is_a(&target.name)
                            && !(matches!(function, Operation::IsSubclassOf)
                                && class.index == target.index)
                    },
                )))
            }
            Operation::GetParentClass | Operation::ClassMethods => {
                let class = class.ok_or_else(|| invalid_class_argument(function, &value))?;
                if matches!(function, Operation::GetParentClass) {
                    Ok(class
                        .parent
                        .as_ref()
                        .map_or(Value::Bool(false), |parent| parent.name_value.clone()))
                } else {
                    self.class_methods(&class)
                }
            }
            Operation::GetClassVars => {
                let class = class.ok_or_else(|| NativeError::type_error(format!(
                    "get_class_vars(): Argument #1 ($class) must be a valid class name, {} given", string(&value)
                )))?;
                self.class_variables(&class, (function.name(), &arguments))
                    .await
            }
            Operation::ClassParents | Operation::ClassImplements | Operation::ClassUses => {
                check_object_or_class(function, &value)?;
                let Some(class) = class else {
                    let suffix = if autoload {
                        " and could not be loaded"
                    } else {
                        ""
                    };
                    self.warn(format!(
                        "{}(): Class {} does not exist{suffix}",
                        function.name(),
                        string(&value)
                    ));
                    return Ok(Value::Bool(false));
                };
                self.class_relations(function, &class)
            }
            Operation::MethodExists | Operation::PropertyExists => {
                check_object_or_class(function, &value)?;
                let name = string(&arguments[1]);
                // A native class answers for properties that live in its
                // own state (DateInterval's `y`, `m`, `d` and so on).
                if matches!(function, Operation::PropertyExists)
                    && let Value::Object(object) = &value
                    && let Some(access) = crate::properties::native_property_access(object)
                    && (access.read)(
                        self,
                        object,
                        &name,
                        phera_extension_api::NativePropertyRead::Exists,
                    )?
                    .is_some()
                {
                    return Ok(Value::Bool(true));
                }
                Ok(Value::Bool(class.is_some_and(|class| {
                    if matches!(function, Operation::PropertyExists) {
                        class.name != phera_extension_api::serialization::INCOMPLETE_CLASS
                            && self.class_property_exists(&value, &class, &name)
                    } else {
                        class
                            .methods
                            .get(&name.to_ascii_lowercase())
                            .is_some_and(|method| {
                                method.visibility != Visibility::Private
                                    || method.declaring == class.name
                                    || value.is_object()
                            })
                            || (class.name == "Closure" && name.eq_ignore_ascii_case("__invoke"))
                    }
                })))
            }
            _ => Err(internal_error!("class introspection function")),
        }
    }

    /// PHP lists the canonical declaration spelling, or the lowercased key for an alias.
    fn declared_classes(&self, function: ClassFunction) -> NativeResult {
        let mut listed = PhpArray::new();
        for name in self.classes.names() {
            if self
                .classes
                .get_lowercase(&name.to_ascii_lowercase())
                .is_some_and(|class| match function {
                    ClassFunction::DeclaredInterfaces => class.kind == ClassKind::Interface,
                    ClassFunction::DeclaredTraits => class.kind == ClassKind::Trait,
                    _ => class.kind.is_class(),
                })
            {
                listed.push(Value::from(name))?;
            }
        }
        Ok(Value::from(listed))
    }
}

impl Vm {
    pub(crate) fn called_class_name(&self) -> NativeResult {
        self.current()
            .and_then(|frame| frame.called())
            .map(|class| class.name_value.clone())
            .ok_or_else(|| {
                NativeError::error("get_called_class() must be called from within a class")
            })
    }
}

fn check_object_or_class(function: ClassFunction, value: &Value) -> Result<(), NativeError> {
    if matches!(value, Value::String(_) | Value::Object(_)) {
        return Ok(());
    }
    Err(NativeError::type_error(format!(
        "{}(): Argument #1 ($object_or_class) must be of type object|string, {} given",
        function.name(),
        value.type_name()
    )))
}

fn invalid_class_argument(function: ClassFunction, value: &Value) -> NativeError {
    NativeError::type_error(format!(
        "{}(): Argument #1 ($object_or_class) must be an object or a valid class name, {} given",
        function.name(),
        value.type_name()
    ))
}

fn string(value: &Value) -> String {
    match value {
        Value::String(name) => name.to_string_lossy(),
        _ => String::new(),
    }
}
