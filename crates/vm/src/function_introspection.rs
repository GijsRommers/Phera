//! `function_exists('strlen')` sees native functions and declarations already made in this request.

use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{Vm, calls::callback_binding::ArgumentOrigin};

#[derive(Clone, Copy)]
pub(crate) enum RuntimeFunction {
    DefinedFunctions,
    ExtensionLoaded,
    LoadedExtensions,
    PhpVersion,
}

impl RuntimeFunction {
    pub(crate) const ALL: &[Self] = &[
        Self::DefinedFunctions,
        Self::ExtensionLoaded,
        Self::LoadedExtensions,
        Self::PhpVersion,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::DefinedFunctions => "get_defined_functions",
            Self::ExtensionLoaded => "extension_loaded",
            Self::LoadedExtensions => "get_loaded_extensions",
            Self::PhpVersion => "phpversion",
        }
    }

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.name() == name)
    }

    pub(crate) fn parameters(self) -> &'static [Param] {
        match self {
            Self::DefinedFunctions => {
                const { &[Param::new("exclude_disabled", ParamType::Bool).optional()] }
            }
            Self::ExtensionLoaded => const { &[Param::new("extension", ParamType::String)] },
            Self::LoadedExtensions => {
                const { &[Param::new("zend_extensions", ParamType::Bool).optional()] }
            }
            Self::PhpVersion => {
                const {
                    &[Param::new("extension", ParamType::String)
                        .optional()
                        .nullable()]
                }
            }
        }
    }
}

pub(crate) fn deprecated(name: &str, since: &str) -> NativeError {
    NativeError::throw(
        "Phera\\NotSupportedException",
        format!("{name} is not supported by Phera: deprecated since PHP {since}"),
    )
}

impl Vm {
    pub(crate) fn runtime_function(
        &mut self,
        function: RuntimeFunction,
        mut arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(
                function.name(),
                function.parameters(),
                &mut arguments,
                vm,
            )
        })?;
        let extension = match arguments.first() {
            Some(Value::String(name)) => Some(name.to_string_lossy()),
            _ => None,
        };
        let loaded = extension.as_ref().is_some_and(|name| {
            self.registry
                .extensions()
                .any(|extension| extension.eq_ignore_ascii_case(name))
        });
        Ok(match function {
            RuntimeFunction::ExtensionLoaded => Value::Bool(loaded),
            RuntimeFunction::LoadedExtensions => {
                let mut result = PhpArray::new();
                if !arguments.first().is_some_and(phera_value::to_bool) {
                    for name in self.registry.extensions() {
                        result.push(Value::from(name))?;
                    }
                }
                Value::from(result)
            }
            RuntimeFunction::PhpVersion => {
                if extension.is_none() || loaded {
                    Value::from(phera_extension_api::PHP_VERSION)
                } else {
                    Value::Bool(false)
                }
            }
            RuntimeFunction::DefinedFunctions => {
                if !arguments.is_empty() {
                    return Err(deprecated(
                        "get_defined_functions() with $exclude_disabled",
                        "8.0",
                    ));
                }
                self.defined_functions()?
            }
        })
    }

    fn defined_functions(&self) -> NativeResult {
        let mut internal = PhpArray::new();
        let registered = self.registry.functions().map(|entry| entry.name);
        for name in registered.chain(crate::intrinsics::Intrinsic::names()) {
            if self.function_table.contains(name) {
                continue;
            }
            internal.push(Value::from(name.to_ascii_lowercase()))?;
        }
        let mut user = PhpArray::new();
        for function in self.function_table.declaration_order() {
            user.push(Value::from(
                self.code.function(function).name.to_ascii_lowercase(),
            ))?;
        }
        let mut result = PhpArray::new();
        result.insert(ArrayKey::str("internal"), Value::from(internal));
        result.insert(ArrayKey::str("user"), Value::from(user));
        Ok(Value::from(result))
    }
}

impl Vm {
    /// Constant functions share the class loader; only the language syntax recognizes `::class`.
    pub(crate) async fn name_intrinsic(
        &mut self,
        intrinsic: crate::intrinsics::Intrinsic,
        mut arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        use crate::intrinsics::Intrinsic;
        let (function, parameter) = match intrinsic {
            Intrinsic::FunctionExists => ("function_exists", "function"),
            Intrinsic::Defined => ("defined", "constant_name"),
            _ => ("constant", "name"),
        };
        self.with_argument_origin(origin, |vm| {
            let params = [Param::new(parameter, ParamType::String)];
            phera_extension_api::prepare_call(function, &params, &mut arguments, vm)
        })?;
        let Some(Value::String(name)) = arguments.first() else {
            return Err(internal_error!("constant or function name"));
        };
        let name = name.to_string_lossy();
        if matches!(intrinsic, Intrinsic::FunctionExists) {
            return Ok(Value::Bool(
                self.exists(name.strip_prefix('\\').unwrap_or(&name)),
            ));
        }
        let predicate = matches!(intrinsic, Intrinsic::Defined);
        if let Some((class, constant)) = name.rsplit_once("::") {
            let class = match self.relative_constant_scope(class) {
                Ok(Some(class)) => class,
                Ok(None) => class.to_owned(),
                Err(_) if predicate => return Ok(Value::Bool(false)),
                Err(error) => return Err(error),
            };
            let found = self.lookup_class(&class).await?;
            let result = if let Some(found) = found {
                if constant.eq_ignore_ascii_case("class") {
                    Err(NativeError::error(format!(
                        "Undefined constant {}::{constant}",
                        found.name
                    )))
                } else {
                    self.class_constant(&class, constant)
                }
            } else {
                Err(NativeError::error(format!("Class \"{class}\" not found")))
            };
            return if predicate {
                Ok(Value::Bool(result.is_ok()))
            } else {
                result
            };
        }
        if predicate {
            Ok(Value::Bool(self.constants.contains_key(&name)))
        } else {
            self.constants
                .get(&name)
                .cloned()
                .ok_or_else(|| NativeError::error(format!("Undefined constant \"{name}\"")))
        }
    }

    fn relative_constant_scope(&self, class: &str) -> Result<Option<String>, NativeError> {
        let frame = self.current();
        let resolved = match class.to_ascii_lowercase().as_str() {
            "self" => frame.and_then(|frame| frame.scope().cloned()),
            "static" => frame.and_then(|frame| frame.called()),
            "parent" => frame.and_then(|frame| frame.scope()?.parent.clone()),
            _ => return Ok(None),
        };
        resolved
            .map(|class| Some(class.name.clone()))
            .ok_or_else(|| {
                NativeError::error(format!(
                    "Cannot access \"{class}\" when no class scope is active"
                ))
            })
    }
}
