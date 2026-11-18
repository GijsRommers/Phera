//! Intrinsics inspect the caller scope and request state directly.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::{Vm, calls::callback_binding::ArgumentOrigin, intrinsics::Intrinsic};

impl Vm {
    /// Callback entry preserves weak scalar binding without changing the PHP body mode.
    pub(crate) fn intrinsic_origin(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        if matches!(intrinsic, Intrinsic::ArrayMap) {
            Box::pin(self.map_arrays(arguments))
        } else {
            Box::pin(self.request_intrinsic(intrinsic, arguments, origin))
        }
    }

    /// Other request services may suspend while loading classes or entering a callback.
    async fn request_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        let argument = arguments.first().cloned().unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        match intrinsic {
            Intrinsic::ParentProperty(operation) => {
                diagnostics
                    .vm
                    .parent_property_fallback(operation, arguments)
                    .await
            }
            Intrinsic::Clone => diagnostics.vm.clone_intrinsic(arguments).await,
            Intrinsic::ArrayMap => Err(internal_error!("array map intrinsic routing")),
            Intrinsic::Handler(function) => {
                diagnostics
                    .vm
                    .handler_intrinsic(function, arguments, origin)
                    .await
            }
            Intrinsic::Basename => {
                let path = to_php_string(&argument, &mut diagnostics.buffer)?;
                let path = path.as_bytes();
                let separator = |byte| byte == b'/' || (cfg!(windows) && byte == b'\\');
                let end = path
                    .iter()
                    .rposition(|&byte| !separator(byte))
                    .map_or(0, |at| at + 1);
                let name = path[..end]
                    .rsplit(|&byte| separator(byte))
                    .next()
                    .unwrap_or_default();
                let suffix = arguments
                    .get(1)
                    .map(|value| to_php_string(value, &mut diagnostics.buffer))
                    .transpose()?;
                let name = suffix
                    .as_ref()
                    .filter(|suffix| suffix.as_bytes().len() < name.len())
                    .and_then(|suffix| name.strip_suffix(suffix.as_bytes()))
                    .unwrap_or(name);
                Ok(Value::from(name))
            }
            Intrinsic::Class(function) => {
                drop(diagnostics);
                Box::pin(self.class_intrinsic(function, arguments, origin)).await
            }
            Intrinsic::SplAutoload(function) => {
                drop(diagnostics);
                self.spl_autoload_intrinsic(function, arguments, origin)
                    .await
            }
            Intrinsic::Runtime(function) => {
                drop(diagnostics);
                self.runtime_function(function, arguments, origin)
            }
            Intrinsic::GetIncludePath | Intrinsic::SetIncludePath => {
                drop(diagnostics);
                self.include_path_intrinsic(intrinsic, arguments, origin)
            }
            Intrinsic::IsCallable => {
                drop(diagnostics);
                Box::pin(self.callable_introspection(arguments, origin)).await
            }
            Intrinsic::CallUserFunc => {
                diagnostics.vm.autoload_callable(&argument).await?;
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
            Intrinsic::Defined | Intrinsic::Constant | Intrinsic::FunctionExists => {
                drop(diagnostics);
                Box::pin(self.name_intrinsic(intrinsic, arguments, origin)).await
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
        self.autoload_callable(&argument).await?;
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
