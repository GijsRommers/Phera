//! `function_exists('strlen')` sees native functions and declarations already made in this request.

use phera_extension_api::{NativeError, NativeResult, Param, ParamType};
use phera_source::deprecations::ItemKind;
use phera_value::{ArrayKey, PhpArray, Value};

use crate::class_constants::ConstantAccess;
use crate::{Vm, calls::callback_binding::ArgumentOrigin};

#[derive(Clone, Copy)]
pub(crate) enum RuntimeFunction {
    DefinedFunctions,
    DefinedConstants,
    ExtensionLoaded,
    ExtensionFunctions,
    LoadedExtensions,
    PhpVersion,
}

impl RuntimeFunction {
    pub(crate) const ALL: &[Self] = &[
        Self::DefinedFunctions,
        Self::DefinedConstants,
        Self::ExtensionLoaded,
        Self::ExtensionFunctions,
        Self::LoadedExtensions,
        Self::PhpVersion,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::DefinedFunctions => "get_defined_functions",
            Self::DefinedConstants => "get_defined_constants",
            Self::ExtensionLoaded => "extension_loaded",
            Self::ExtensionFunctions => "get_extension_funcs",
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
            Self::DefinedConstants => {
                const { &[Param::new("categorize", ParamType::Bool).optional()] }
            }
            Self::ExtensionLoaded | Self::ExtensionFunctions => {
                const { &[Param::new("extension", ParamType::String)] }
            }
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
        crate::unsupported_items::EXCEPTION,
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
            RuntimeFunction::ExtensionFunctions => {
                let Some(name) = extension.as_deref() else {
                    return Ok(Value::Bool(false));
                };
                if name.eq_ignore_ascii_case("zend") {
                    return self.extension_functions("core");
                }
                if !loaded {
                    return Ok(Value::Bool(false));
                }
                return self.extension_functions(name);
            }
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
                // A module that ships with PHP reports PHP's version; one
                // with a release of its own (phpredis) reports that.
                match &extension {
                    None => Value::from(phera_extension_api::PHP_VERSION),
                    Some(name) if loaded => Value::from(
                        self.registry
                            .module_version(name)
                            .unwrap_or(phera_extension_api::PHP_VERSION),
                    ),
                    Some(_) => Value::Bool(false),
                }
            }
            RuntimeFunction::DefinedFunctions => {
                if !arguments.is_empty() {
                    self.diagnostic("Deprecated", "get_defined_functions(): The $exclude_disabled parameter has no effect since PHP 8.0".into());
                }
                self.defined_functions()?
            }
            RuntimeFunction::DefinedConstants => {
                self.defined_constants(arguments.first().is_some_and(phera_value::to_bool))
            }
        })
    }

    fn extension_functions(&self, module: &str) -> NativeResult {
        let functions = include_str!("../../extension-api/data/extension-functions.tsv");
        let mut result = PhpArray::new();
        for (owner, name) in functions.lines().filter_map(|row| row.split_once('\t')) {
            if owner.eq_ignore_ascii_case(module)
                && (self.registry.get_function(name).is_some()
                    || crate::intrinsics::Intrinsic::resolve(name).is_some())
            {
                result.push(Value::from(name))?;
            }
        }
        Ok(if result.is_empty() {
            Value::Bool(false)
        } else {
            Value::from(result)
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
        let name = name.strip_prefix('\\').unwrap_or(&name);
        let predicate = matches!(intrinsic, Intrinsic::Defined);
        if let Some((class, constant)) = name.rsplit_once("::") {
            let written = class;
            let class = self
                .relative_constant_scope(class)?
                .unwrap_or_else(|| written.to_owned());
            let origin = Some((function, arguments.as_slice()));
            let Some(entry) = self.lookup_class_from(&class, origin).await? else {
                return if predicate {
                    Ok(Value::Bool(false))
                } else {
                    Err(crate::unsupported_items::omitted_member(
                        ItemKind::ClassConstant,
                        &class,
                        constant,
                    )
                    .unwrap_or_else(|| NativeError::error(format!("Class \"{class}\" not found"))))
                };
            };
            let accessible = entry.constants.get(constant).is_some_and(|constant| {
                entry.kind != phera_bytecode::ClassKind::Trait
                    && self.can_read_class_constant(constant)
            });
            if predicate && !accessible {
                return Ok(Value::Bool(false));
            }
            if !entry.constants.contains_key(constant) {
                return Err(crate::unsupported_items::omitted_native_member(
                    ItemKind::ClassConstant,
                    &entry,
                    constant,
                )
                .unwrap_or_else(|| {
                    NativeError::error(format!("Undefined constant {written}::{constant}"))
                }));
            }
            let access = if predicate {
                ConstantAccess::Defined
            } else {
                ConstantAccess::Read
            };
            loop {
                match self.class_constant_with_access(&class, constant, access) {
                    Ok(value) => return Ok(if predicate { Value::Bool(true) } else { value }),
                    Err(error) if entry.kind == phera_bytecode::ClassKind::Trait => {
                        self.failed_native =
                            Some(crate::error_traces::NativeTrace::new(function, &arguments));
                        return Err(error);
                    }
                    Err(error) => {
                        let miss = crate::autoload::ClassMiss::take(error)?;
                        if self.lookup_class_from(&miss.name, origin).await?.is_none() {
                            return Err(miss.error());
                        }
                    }
                }
            }
        }
        let special = match name.to_ascii_lowercase().as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            "null" => Some(Value::Null),
            _ => None,
        };
        if let Some(value) = special {
            return Ok(if predicate { Value::Bool(true) } else { value });
        }
        if predicate {
            Ok(Value::Bool(self.constants.contains_key(name)))
        } else {
            let constant = self.constants.get(name).cloned().ok_or_else(|| {
                crate::unsupported_items::omitted_item(ItemKind::Constant, name)
                    .unwrap_or_else(|| NativeError::error(format!("Undefined constant \"{name}\"")))
            })?;
            Ok(self.report_global_constant(constant))
        }
    }

    fn relative_constant_scope(&self, class: &str) -> Result<Option<String>, NativeError> {
        let frame = self.current();
        let resolved = match class.to_ascii_lowercase().as_str() {
            "self" => frame.and_then(|frame| frame.scope().cloned()),
            "static" => frame.and_then(|frame| frame.called()),
            "parent" => {
                let scope = frame.and_then(|frame| frame.scope());
                if scope.is_some_and(|scope| scope.parent.is_none()) {
                    return Err(NativeError::error(
                        "Cannot access \"parent\" when current class scope has no parent",
                    ));
                }
                scope.and_then(|scope| scope.parent.clone())
            }
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
