//! Class introspection reads linked metadata without triggering autoloading.

use crate::Vm;
use phera_bytecode::ClassKind;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};
use std::rc::Rc;

/// Class functions are resolved once, alongside other request intrinsics.
#[derive(Clone, Copy)]
pub(crate) enum ClassFunction {
    GetClass,
    GetParentClass,
    GetCalledClass,
    MethodExists,
    ClassExists,
    InterfaceExists,
    IsA,
    IsSubclassOf,
    ObjectId,
    ObjectHash,
    ClassMethods,
    ClassParents,
    ClassImplements,
    DeclaredClasses,
}

impl ClassFunction {
    /// PHP builtin function identifiers are case insensitive.
    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Some(match name {
            "get_class" => Self::GetClass,
            "get_parent_class" => Self::GetParentClass,
            "get_called_class" => Self::GetCalledClass,
            "method_exists" => Self::MethodExists,
            "class_exists" => Self::ClassExists,
            "interface_exists" => Self::InterfaceExists,
            "is_a" => Self::IsA,
            "is_subclass_of" => Self::IsSubclassOf,
            "spl_object_id" => Self::ObjectId,
            "spl_object_hash" => Self::ObjectHash,
            "get_class_methods" => Self::ClassMethods,
            "class_parents" => Self::ClassParents,
            "class_implements" => Self::ClassImplements,
            "get_declared_classes" => Self::DeclaredClasses,
            _ => return None,
        })
    }
}

impl Vm {
    /// Extract class identifiers without converting arbitrary values to strings.
    fn introspection_class(value: &Value) -> Option<String> {
        match &*value.deref() {
            Value::Object(object) => Some(object.class.name().to_owned()),
            Value::String(name) => Some(name.to_string_lossy()),
            _ => None,
        }
    }

    /// Class predicates distinguish existence from ancestry and never instantiate anything.
    pub(crate) fn class_intrinsic(
        &mut self,
        function: ClassFunction,
        arguments: &[Value],
    ) -> NativeResult {
        use ClassFunction as Function;
        let value = arguments.first().cloned().unwrap_or_default();
        let name = Self::introspection_class(&value);
        let class = name.as_ref().and_then(|name| self.classes.get(name));
        match function {
            Function::GetClass => {
                if let Value::Object(object) = value {
                    return Ok(Value::from(object.class.name().to_owned()));
                }
                if arguments.is_empty() {
                    return self
                        .current()
                        .and_then(|frame| frame.method.as_ref())
                        .map(|context| Value::from(context.scope.name.clone()))
                        .ok_or_else(|| {
                            NativeError::error(
                                "get_class() without arguments must be called from within a class",
                            )
                        });
                }
                Err(NativeError::type_error(format!(
                    "get_class(): Argument #1 ($object) must be of type object, {} given",
                    value.type_name()
                )))
            }
            Function::GetParentClass => {
                let class = class.or_else(|| {
                    if arguments.is_empty() {
                        self.current()
                            .and_then(|frame| frame.method.as_ref())
                            .map(|context| context.scope.clone())
                    } else {
                        None
                    }
                });
                Ok(class
                    .and_then(|class| class.parent.clone())
                    .map(|parent| Value::from(parent.name.clone()))
                    .unwrap_or(Value::Bool(false)))
            }
            Function::GetCalledClass => self
                .current()
                .and_then(|frame| frame.method.as_ref())
                .map(|context| Value::from(context.called.name.clone()))
                .ok_or_else(|| {
                    NativeError::error("get_called_class() must be called from within a class")
                }),
            Function::ClassExists | Function::InterfaceExists => {
                Ok(Value::Bool(class.is_some_and(|class| {
                    if matches!(function, Function::InterfaceExists) {
                        class.kind == ClassKind::Interface
                    } else {
                        class.kind != ClassKind::Interface
                    }
                })))
            }
            Function::MethodExists => {
                let method = arguments
                    .get(1)
                    .and_then(Self::introspection_class)
                    .unwrap_or_default();
                Ok(Value::Bool(class.is_some_and(|class| {
                    class.methods.contains_key(&method.to_ascii_lowercase())
                })))
            }
            Function::IsA | Function::IsSubclassOf => {
                let allow_string = arguments
                    .get(2)
                    .map(phera_value::to_bool)
                    .unwrap_or(matches!(function, Function::IsSubclassOf));
                let target = arguments
                    .get(1)
                    .and_then(Self::introspection_class)
                    .unwrap_or_default();
                Ok(Value::Bool(
                    (value.is_object() || allow_string)
                        && class.is_some_and(|class| {
                            class.is_a(&target)
                                && (!matches!(function, Function::IsSubclassOf)
                                    || !class.name.eq_ignore_ascii_case(&target))
                        }),
                ))
            }
            Function::ObjectId | Function::ObjectHash => {
                let Value::Object(object) = value else {
                    let name = if matches!(function, Function::ObjectId) {
                        "spl_object_id"
                    } else {
                        "spl_object_hash"
                    };
                    return Err(NativeError::type_error(format!(
                        "{name}(): Argument #1 ($object) must be of type object, {} given",
                        value.type_name()
                    )));
                };
                Ok(if matches!(function, Function::ObjectId) {
                    Value::Int(object.id.into())
                } else {
                    Value::from(format!("{:016x}{:016x}", object.id, 0))
                })
            }
            _ => self.class_listing(function, class),
        }
    }

    /// Visibility-aware listings expose only members callable from the active lexical scope.
    fn class_listing(
        &mut self,
        function: ClassFunction,
        class: Option<Rc<crate::classes::Class>>,
    ) -> NativeResult {
        use ClassFunction as Function;
        let mut result = PhpArray::new();
        if matches!(function, Function::DeclaredClasses) {
            for name in &self.classes.order {
                if self
                    .classes
                    .get(name)
                    .is_some_and(|class| class.kind != ClassKind::Interface)
                {
                    result.push(Value::from(name.clone()))?;
                }
            }
            return Ok(Value::Array(Rc::new(result)));
        }
        let Some(class) = class else {
            return Ok(Value::Bool(false));
        };
        match function {
            Function::ClassMethods => {
                let mut methods: Vec<_> = class.methods.values().collect();
                methods.sort_by_key(|method| self.method_listing_order(&class, method));
                for method in methods {
                    if self.check_method_visibility(method).is_ok() {
                        result.push(Value::from(method.name.clone()))?;
                    }
                }
            }
            Function::ClassParents => {
                let mut parent = class.parent.clone();
                while let Some(class) = parent {
                    result.insert(
                        ArrayKey::str(class.name.as_str()),
                        Value::from(class.name.clone()),
                    );
                    parent = class.parent.clone();
                }
            }
            Function::ClassImplements => {
                let mut pending = vec![class];
                while let Some(class) = pending.pop() {
                    for interface in &class.interfaces {
                        result.insert(
                            ArrayKey::str(interface.name.as_str()),
                            Value::from(interface.name.clone()),
                        );
                        pending.push(interface.clone());
                    }
                    pending.extend(class.parent.clone());
                }
            }
            _ => return Err(internal_error!("class listing")),
        }
        Ok(Value::Array(Rc::new(result)))
    }
}

/// PHP lists declared methods before inherited methods, retaining source order within each class.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct MethodOrder {
    /// Zero denotes the requested class, then each successive parent.
    depth: usize,
    /// User function order or the native registration position.
    declaration: usize,
}

impl Vm {
    /// A method's declaring class determines its position independently of hash table order.
    fn method_listing_order(
        &self,
        class: &Rc<crate::classes::Class>,
        method: &crate::classes::Method,
    ) -> MethodOrder {
        let mut ancestor = Some(class.clone());
        let mut depth = 0;
        while let Some(class) = ancestor {
            if class.name == method.declaring {
                break;
            }
            depth += 1;
            ancestor = class.parent.clone();
        }
        let declaration = match &method.body {
            crate::classes::MethodBody::User(reference) => reference.index,
            crate::classes::MethodBody::Native(_) => self
                .registry
                .classes()
                .find(|class| class.name == method.declaring)
                .and_then(|class| {
                    class
                        .methods
                        .iter()
                        .position(|entry| entry.name == method.name)
                })
                .unwrap_or(0),
        };
        MethodOrder { depth, declaration }
    }
}
