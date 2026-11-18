//! Intrinsics inspect the caller scope and request state directly.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::{Vm, calls::callback_binding::ArgumentOrigin, intrinsics::Intrinsic};

impl Vm {
    /// Callback entry preserves weak scalar binding without changing the PHP body mode.
    pub(crate) async fn intrinsic_origin(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        let argument = arguments.first().cloned().unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        match intrinsic {
            Intrinsic::ArrayMap => diagnostics.vm.map_arrays(arguments).await,
            Intrinsic::Handler(function) => {
                diagnostics
                    .vm
                    .handler_intrinsic(function, arguments, origin)
                    .await
            }
            Intrinsic::Basename => {
                let path = to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy();
                let name = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
                let suffix = arguments
                    .get(1)
                    .map(|value| to_php_string(value, &mut diagnostics.buffer))
                    .transpose()?;
                let name = suffix
                    .as_ref()
                    .and_then(|suffix| name.strip_suffix(suffix.to_string_lossy().as_str()))
                    .unwrap_or(name);
                Ok(Value::from(name))
            }
            Intrinsic::Class(function) => diagnostics.vm.class_intrinsic(function, &arguments),
            Intrinsic::GetObjectVars => {
                let Value::Object(object) = argument else {
                    return Err(NativeError::type_error(
                        "get_object_vars(): Argument #1 ($object) must be of type object",
                    ));
                };
                Ok(Value::Array(Rc::new(diagnostics.vm.visible_properties(
                    &object,
                    phera_extension_api::PropertyPurpose::ObjectVars,
                )?)))
            }
            Intrinsic::GetClassVars => {
                let name = to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy();
                diagnostics.vm.class_variables(&name)
            }
            Intrinsic::PropertyExists => {
                let name = to_php_string(
                    arguments.get(1).unwrap_or(&Value::Null),
                    &mut diagnostics.buffer,
                )?
                .to_string_lossy();
                let (class_name, dynamic) = match &argument {
                    Value::Object(object) => (
                        object.class.name().to_owned(),
                        object.has(&phera_value::ArrayKey::str(name.as_bytes())),
                    ),
                    Value::String(class) => (class.to_string_lossy(), false),
                    _ => {
                        return Err(NativeError::type_error(
                            "property_exists(): Argument #1 ($object_or_class) must be of \
                             type object|string",
                        ));
                    }
                };
                Ok(Value::Bool(
                    dynamic
                        || diagnostics
                            .vm
                            .classes
                            .get(&class_name)
                            .is_some_and(|class| {
                                class
                                    .defaults
                                    .iter()
                                    .chain(&class.static_properties)
                                    .any(|property| property.name == name)
                            }),
                ))
            }
            Intrinsic::IsCallable => diagnostics
                .vm
                .with_argument_origin(origin, |vm| vm.callable_introspection(arguments)),
            Intrinsic::CallUserFunc => {
                let callee = diagnostics
                    .vm
                    .resolve_callable(&argument)
                    .map_err(|error| {
                        diagnostics.vm.invalid_callback(
                            "call_user_func",
                            1,
                            "callback",
                            &argument,
                            error,
                            false,
                        )
                    })?;
                return diagnostics
                    .vm
                    .invoke_callable_origin(callee, arguments.into_iter().skip(1), origin)
                    .await;
            }
            Intrinsic::CallUserFuncArray => {
                diagnostics
                    .vm
                    .call_user_func_array(argument, arguments, origin)
                    .await
            }
            Intrinsic::GetDefinedVars => diagnostics.vm.defined_variables(),
            Intrinsic::Compact => diagnostics.vm.compact_variables(arguments),
            Intrinsic::Extract => diagnostics.vm.extract_variables(arguments),
            Intrinsic::Define => {
                diagnostics
                    .vm
                    .define_argument(argument, arguments, &mut diagnostics.buffer)
            }
            Intrinsic::Defined | Intrinsic::Constant => {
                let name = to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy();
                if let Some((class, constant)) = name.split_once("::") {
                    let result = diagnostics.vm.class_constant(class, constant);
                    if matches!(intrinsic, Intrinsic::Defined) {
                        Ok(Value::Bool(result.is_ok()))
                    } else {
                        result
                    }
                } else if matches!(intrinsic, Intrinsic::Defined) {
                    Ok(Value::Bool(diagnostics.vm.constants.contains_key(&name)))
                } else {
                    diagnostics
                        .vm
                        .constants
                        .get(&name)
                        .cloned()
                        .ok_or_else(|| NativeError::error(format!("Undefined constant \"{name}\"")))
                }
            }
            Intrinsic::FunctionExists => {
                Ok(Value::Bool(diagnostics.vm.exists(
                    &to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy(),
                )))
            }
            Intrinsic::FuncNumArgs => diagnostics.vm.argument_intrinsic(
                crate::calls::argument_errors::ArgumentFunction::Count,
                arguments,
            ),
            Intrinsic::FuncGetArgs => diagnostics.vm.argument_intrinsic(
                crate::calls::argument_errors::ArgumentFunction::All,
                arguments,
            ),
            Intrinsic::FuncGetArg => diagnostics.vm.argument_intrinsic(
                crate::calls::argument_errors::ArgumentFunction::One,
                arguments,
            ),
            Intrinsic::ErrorReporting => diagnostics.vm.error_reporting_intrinsic(arguments),
            Intrinsic::IniSet | Intrinsic::IniGet => {
                diagnostics.vm.exception_ini_setting(intrinsic, &arguments)
            }
        }
    }
}

impl Vm {
    /// Callback arguments are copied from the PHP array before entering the callable.
    async fn call_user_func_array(
        &mut self,
        argument: Value,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        let array = arguments.get(1).cloned().unwrap_or_default();
        let Value::Array(array) = array else {
            return Err(NativeError::type_error(
                "call_user_func_array(): Argument #2 ($args) must be of type array",
            ));
        };
        let callee = self.resolve_callable(&argument).map_err(|error| {
            self.invalid_callback(
                "call_user_func_array",
                1,
                "callback",
                &argument,
                error,
                false,
            )
        })?;
        self.invoke_array_callback_origin(callee, &array, origin)
            .await
    }
}

impl Vm {
    /// Duplicate define calls warn and leave the existing request constant unchanged.
    fn define_argument(
        &mut self,
        argument: Value,
        arguments: Vec<Value>,
        diagnostics: &mut phera_value::CollectDiagnostics,
    ) -> NativeResult {
        let name = to_php_string(&argument, diagnostics)?.to_string_lossy();
        if name.contains("::") {
            self.failed_native = Some(crate::error_traces::NativeTrace::new("define", &arguments));
            return Err(NativeError::value_error(
                "define(): Argument #1 ($constant_name) cannot be a class constant",
            ));
        }
        if self.constants.contains_key(&name) {
            self.warn(format!("Constant {name} already defined"));
            return Ok(Value::Bool(false));
        }
        self.constants
            .insert(name, arguments.get(1).cloned().unwrap_or_default());
        Ok(Value::Bool(true))
    }
}
