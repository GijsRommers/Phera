//! `json_decode($json, flags: JSON_THROW_ON_ERROR)` leaves two slots to fill.
//! All callable forms route to this binder when finishing a native call window.

use phera_extension_api::{
    NativeError, NativeResult, Param,
    native_parameters::{StubDefault, signature},
};
use phera_value::Value;

use crate::Vm;
use crate::classes::MethodBody;
use crate::error_traces::NativeTrace;

use super::call_state::{CallTarget, Callee};

impl Vm {
    #[cold]
    pub(super) fn fill_native_gap(&mut self, callee: &Callee, index: usize) -> NativeResult {
        let (name, parameters) = match &callee.target {
            CallTarget::Native(entry) => (entry.name, Some(entry.params)),
            CallTarget::Method { method, .. } => match method.body {
                MethodBody::Native(entry) => {
                    (&*method.signature(&self.code).name, Some(entry.params))
                }
                _ => (&*callee.name, None),
            },
            CallTarget::Intrinsic(intrinsic) => (&*callee.name, intrinsic.parameters()),
            _ => (&*callee.name, None),
        };
        if let Some(parameter) = signature(name).and_then(|stub| stub.parameters().nth(index)) {
            return match parameter.default() {
                Some(StubDefault::EnumCase(class, case)) => self.class_constant(class, case),
                Some(default) => default
                    .literal()
                    .ok_or_else(|| internal_error!("native parameter default")),
                None if parameter.variadic => Ok(Value::Null),
                None => Err(gap_error(name, index, parameter.name, parameter.optional)),
            };
        }

        // Extensions outside php-src can declare their own defaults.
        if let Some(&Param {
            name: parameter,
            default,
            variadic: false,
            optional,
            ..
        }) = parameters.and_then(|params| params.get(index))
        {
            return default.map_or_else(
                || Err(gap_error(name, index, parameter, optional)),
                |default| Ok(default.to_value()),
            );
        }
        Ok(Value::Null)
    }

    #[cold]
    pub(super) fn record_native_gap_error(&mut self, callee: &Callee, arguments: &[Value]) {
        self.failed_native = match &callee.target {
            CallTarget::Method { method, .. } => Some(NativeTrace::method(method, arguments)),
            _ => Some(self.native_function_trace(&callee.name, arguments)),
        };
    }
}

fn gap_error(name: &str, position: usize, parameter: &str, optional: bool) -> NativeError {
    let reason = if optional {
        "must be passed explicitly, because the default value is not known"
    } else {
        "not passed"
    };
    NativeError::argument_count_error(format!(
        "{name}(): Argument #{} (${parameter}) {reason}",
        position + 1
    ))
}
