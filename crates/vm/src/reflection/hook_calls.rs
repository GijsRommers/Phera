//! `ReflectionProperty::getValue($object)` can suspend on the getter's PHP frame.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{LocalFuture, NativeError, Unwind};
use phera_value::{Object, PhpArray, Value};

use crate::Vm;
use crate::calls::{
    call_execution::CallEntry,
    call_state::{CallTarget, Callee},
    callback_binding::ArgumentOrigin,
};
use crate::classes::Method;
use crate::code_table::FunctionIndex;
use crate::error_traces::{NativeCallbackSite, NativeCaller, NativeTrace};
use crate::property_hook_continuation::HookContinuation;

pub(super) enum ReflectedHookCall {
    Property {
        function: FunctionIndex,
        object: Rc<Object>,
        value: Option<Value>,
    },
    Method {
        callee: Callee,
        args: PhpArray,
    },
}

impl ReflectedHookCall {
    pub(super) fn suspend(self) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self)))
    }

    fn take(error: NativeError) -> Result<Self, NativeError> {
        let NativeError::Unwind(payload) = error else {
            return Err(error);
        };
        payload
            .0
            .downcast::<Self>()
            .map(|call| *call)
            .map_err(|payload| NativeError::Unwind(Unwind(payload)))
    }

    fn trace(&self, method: &Method) -> NativeTrace {
        let values = match self {
            Self::Property { object, value, .. } => {
                let mut values = vec![Value::Object(object.clone())];
                values.extend(value.iter().cloned());
                values
            }
            Self::Method { callee, args } => {
                let receiver = match &callee.target {
                    CallTarget::Method { context, .. } => context.receiver.clone(),
                    _ => None,
                };
                let mut values = vec![receiver.map(Value::Object).unwrap_or_default()];
                if method.name == "invokeArgs" {
                    values.push(Value::from(args.clone()));
                } else {
                    values.extend(args.iter().map(|(_, value)| value.clone()));
                }
                values
            }
        };
        NativeTrace::new(&format!("{}->{}", method.declaring, method.name), &values)
    }

    fn prepare(self, vm: &mut Vm) -> Result<(), NativeError> {
        match self {
            Self::Property {
                function,
                object,
                value,
            } => vm.prepare_declared_hook(function, object, value),
            Self::Method { callee, args } => {
                vm.prepare_call(callee)?;
                vm.route_callback_array(&args)
            }
        }
    }
}

impl Vm {
    pub(crate) fn is_reflected_hook(error: &NativeError) -> bool {
        matches!(error, NativeError::Unwind(unwind) if unwind.0.is::<ReflectedHookCall>())
    }

    #[cold]
    pub(crate) fn finish_reflected_hook(
        &mut self,
        error: NativeError,
        method: &Method,
        destination: Option<Reg>,
    ) -> LocalFuture<'_, Result<Option<Value>, NativeError>> {
        let call = ReflectedHookCall::take(error);
        let trace = call.as_ref().ok().map(|call| call.trace(method));
        Box::pin(async move {
            let call = call?;
            let trace = trace.ok_or_else(|| internal_error!("reflected hook trace"))?;
            let base = self.registers.len();
            let depth = self.call_stack.call_depth();
            let routed = call.prepare(self);
            let origin = ArgumentOrigin::Callback;
            if let Some(destination) = destination {
                routed?;
                let mut entry =
                    self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
                if matches!(entry, CallEntry::ConvertStrings) {
                    self.coerce_call_strings(origin).await?;
                    entry = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None))?;
                }
                if !matches!(entry, CallEntry::Php) {
                    return Err(internal_error!("reflected hook call entry"));
                }
                self.call_stack.current_extras_mut().property_hook = Some(Box::new(
                    HookContinuation::ReflectedCall(destination, trace),
                ));
                return Ok(None);
            }

            // Generic native callbacks have no PHP destination to suspend into.
            self.native_callback_sites.push(NativeCallbackSite {
                boundary: self.call_stack.frames().len(),
                native: NativeCaller::Saved(trace),
            });
            let result = self.finish_callback(routed, base, depth, origin).await;
            self.native_callback_sites.pop();
            result.map(|value| Some(value.deref().clone()))
        })
    }
}
