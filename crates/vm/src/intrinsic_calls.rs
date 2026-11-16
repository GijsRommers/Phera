//! Intrinsics inspect the caller scope and request state directly.

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, NativeResult};
use phera_value::{PhpArray, Value, to_php_string};

use crate::{Vm, intrinsics::Intrinsic};

impl Vm {
    /// Execute a resolved intrinsic without matching its function name again.
    pub(crate) async fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<Value>,
    ) -> NativeResult {
        let argument = arguments.first().cloned().unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        match intrinsic {
            Intrinsic::IsCallable => {
                let name = to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy();
                Ok(Value::Bool(diagnostics.vm.exists(&name)))
            }
            Intrinsic::CallUserFunc => {
                let rest = arguments.iter().skip(1).cloned().collect();
                return diagnostics.vm.call(&argument, rest).await;
            }
            Intrinsic::CallUserFuncArray => {
                diagnostics
                    .vm
                    .call_user_func_array(argument, arguments)
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
            Intrinsic::Defined => Ok(Value::Bool(diagnostics.vm.constants.contains_key(
                &to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy(),
            ))),
            Intrinsic::Constant => {
                let name = to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy();
                diagnostics
                    .vm
                    .constants
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| NativeError::error(format!("Undefined constant \"{name}\"")))
            }
            Intrinsic::FunctionExists => {
                Ok(Value::Bool(diagnostics.vm.exists(
                    &to_php_string(&argument, &mut diagnostics.buffer)?.to_string_lossy(),
                )))
            }
            Intrinsic::FuncNumArgs => Ok(Value::Int(
                diagnostics
                    .vm
                    .current()
                    .map_or(0, |frame| frame.arguments.len()) as i64,
            )),
            Intrinsic::FuncGetArgs => diagnostics.vm.observed_arguments(),
            Intrinsic::FuncGetArg => {
                let position = phera_value::to_int(&argument, &mut diagnostics.buffer)?;
                diagnostics.vm.current().and_then(|frame|frame.arguments.get(position as usize)).cloned().ok_or_else(||NativeError::value_error("func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function"))
            }
            Intrinsic::ErrorReporting => Ok(Value::Int(30719)),
            Intrinsic::IniSet | Intrinsic::IniGet => Ok(Value::from("1")),
        }
    }
}

impl Vm {
    /// Callback arguments are copied from the PHP array before entering the callable.
    async fn call_user_func_array(
        &mut self,
        argument: Value,
        arguments: Vec<Value>,
    ) -> NativeResult {
        let array = arguments.get(1).cloned().unwrap_or_default();
        let Value::Array(array) = array else {
            return Err(NativeError::type_error(
                "call_user_func_array(): Argument #2 ($args) must be of type array",
            ));
        };
        return self
            .call(&argument, array.values().cloned().collect())
            .await;
    }
}

impl Vm {
    /// PHP argument introspection copies the saved view into a new ordered array.
    fn observed_arguments(&self) -> NativeResult {
        let array = self
            .current()
            .map(|frame| frame.arguments.clone())
            .unwrap_or_default();
        let mut result = PhpArray::new();
        for value in array {
            result.push(value)?;
        }
        Ok(Value::Array(Rc::new(result)))
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
        if self.constants.contains_key(&name) {
            self.warn(format!("Constant {name} already defined"));
            return Ok(Value::Bool(false));
        }
        self.constants
            .insert(name, arguments.get(1).cloned().unwrap_or_default());
        Ok(Value::Bool(true))
    }
}
