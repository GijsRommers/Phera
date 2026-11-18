//! `class_exists('A')` may load A, whereas `class_exists('A', false)` only checks the table.

use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{PhpArray, Value};

use crate::{Vm, calls::callback_binding::ArgumentOrigin, function_introspection::deprecated};

/// Class functions are resolved once alongside the other request intrinsics.
#[derive(Clone, Copy)]
pub(crate) enum ClassFunction {
    GetClass,
    GetParentClass,
    GetCalledClass,
    GetObjectVars,
    GetClassVars,
    PropertyExists,
    MethodExists,
    ClassExists,
    InterfaceExists,
    TraitExists,
    EnumExists,
    ClassAlias,
    IsA,
    IsSubclassOf,
    ObjectId,
    ObjectHash,
    ClassMethods,
    ClassParents,
    ClassImplements,
    DeclaredClasses,
    DeclaredInterfaces,
}

impl ClassFunction {
    pub(crate) const ALL: &[Self] = &[
        Self::GetClass,
        Self::GetParentClass,
        Self::GetCalledClass,
        Self::GetObjectVars,
        Self::GetClassVars,
        Self::PropertyExists,
        Self::MethodExists,
        Self::ClassExists,
        Self::InterfaceExists,
        Self::TraitExists,
        Self::EnumExists,
        Self::ClassAlias,
        Self::IsA,
        Self::IsSubclassOf,
        Self::ObjectId,
        Self::ObjectHash,
        Self::ClassMethods,
        Self::ClassParents,
        Self::ClassImplements,
        Self::DeclaredClasses,
        Self::DeclaredInterfaces,
    ];

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::GetClass => "get_class",
            Self::GetParentClass => "get_parent_class",
            Self::GetCalledClass => "get_called_class",
            Self::GetObjectVars => "get_object_vars",
            Self::GetClassVars => "get_class_vars",
            Self::PropertyExists => "property_exists",
            Self::MethodExists => "method_exists",
            Self::ClassExists => "class_exists",
            Self::InterfaceExists => "interface_exists",
            Self::TraitExists => "trait_exists",
            Self::EnumExists => "enum_exists",
            Self::ClassAlias => "class_alias",
            Self::IsA => "is_a",
            Self::IsSubclassOf => "is_subclass_of",
            Self::ObjectId => "spl_object_id",
            Self::ObjectHash => "spl_object_hash",
            Self::ClassMethods => "get_class_methods",
            Self::ClassParents => "class_parents",
            Self::ClassImplements => "class_implements",
            Self::DeclaredClasses => "get_declared_classes",
            Self::DeclaredInterfaces => "get_declared_interfaces",
        }
    }

    pub(crate) fn parameters(self) -> &'static [Param] {
        const AUTOLOAD: Param = Param::new("autoload", ParamType::Bool).optional();
        const CLASS: Param = Param::new("class", ParamType::String);
        const OBJECT: Param = Param::new("object", ParamType::Object);
        const OBJECT_OR_CLASS: Param = Param::new("object_or_class", ParamType::Mixed);
        match self {
            Self::GetClass => const { &[OBJECT.optional()] },
            Self::GetObjectVars | Self::ObjectId | Self::ObjectHash => const { &[OBJECT] },
            Self::GetParentClass => const { &[OBJECT_OR_CLASS.optional()] },
            Self::ClassMethods => const { &[OBJECT_OR_CLASS] },
            Self::GetClassVars => const { &[CLASS] },
            Self::ClassExists => const { &[CLASS, AUTOLOAD] },
            Self::InterfaceExists => {
                const { &[Param::new("interface", ParamType::String), AUTOLOAD] }
            }
            Self::TraitExists => const { &[Param::new("trait", ParamType::String), AUTOLOAD] },
            Self::EnumExists => const { &[Param::new("enum", ParamType::String), AUTOLOAD] },
            Self::ClassAlias => {
                const { &[CLASS, Param::new("alias", ParamType::String), AUTOLOAD] }
            }
            Self::ClassParents | Self::ClassImplements => const { &[OBJECT_OR_CLASS, AUTOLOAD] },
            Self::PropertyExists => {
                const { &[OBJECT_OR_CLASS, Param::new("property", ParamType::String)] }
            }
            Self::MethodExists => {
                const { &[OBJECT_OR_CLASS, Param::new("method", ParamType::String)] }
            }
            Self::IsA | Self::IsSubclassOf => {
                const {
                    &[
                        OBJECT_OR_CLASS,
                        CLASS,
                        Param::new("allow_string", ParamType::Bool).optional(),
                    ]
                }
            }
            Self::GetCalledClass | Self::DeclaredClasses | Self::DeclaredInterfaces => &[],
        }
    }
}

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
            Operation::DeclaredClasses | Operation::DeclaredInterfaces => {
                return self.declared_classes(function);
            }
            Operation::GetCalledClass => {
                return self
                    .current()
                    .and_then(|frame| frame.called())
                    .map(|class| class.name_value.clone())
                    .ok_or_else(|| {
                        NativeError::error("get_called_class() must be called from within a class")
                    });
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
            | Operation::ObjectId
            | Operation::ObjectHash => {
                let Value::Object(object) = value else {
                    return Err(internal_error!("class function object"));
                };
                return Ok(match function {
                    Operation::GetClass => Value::from(object.class.name()),
                    Operation::GetObjectVars => Value::from(self.visible_properties(&object)),
                    Operation::ObjectId => Value::Int(object.id.into()),
                    _ => Value::from(format!("{:016x}{:016x}", object.id, 0)),
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
            | Operation::ClassImplements => option(1, true),
            _ => true,
        };
        let class = match &value {
            Value::Object(object) => self.classes.for_object(object),
            Value::String(name) => {
                let name = name.to_string_lossy();
                if autoload {
                    self.lookup_class(&name).await?
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
                class.is_some_and(|class| class.kind == ClassKind::Class),
            )),
            Operation::InterfaceExists => Ok(Value::Bool(
                class.is_some_and(|class| class.kind == ClassKind::Interface),
            )),
            // The class table has only class and interface entries until traits and enums are linked.
            Operation::TraitExists | Operation::EnumExists => Ok(Value::Bool(false)),
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
                self.class_variables(&class)
            }
            Operation::ClassParents | Operation::ClassImplements => {
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
                Ok(Value::Bool(class.is_some_and(|class| {
                    if matches!(function, Operation::PropertyExists) {
                        self.class_property_exists(&value, &class, &name)
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
        let wanted = if matches!(function, ClassFunction::DeclaredInterfaces) {
            ClassKind::Interface
        } else {
            ClassKind::Class
        };
        let mut listed = PhpArray::new();
        for name in &self.classes.order {
            if self
                .classes
                .get(name)
                .is_some_and(|class| class.kind == wanted)
            {
                listed.push(Value::from(name.as_str()))?;
            }
        }
        Ok(Value::from(listed))
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
