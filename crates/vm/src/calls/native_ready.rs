//! A native method may complete directly from a prepared argument window.
//!
//! ```php
//! $generator->send($value);
//! ```
//!
//! Positional, untyped arguments already hold their PHP values. Missing
//! arguments, named arguments, and methods that need to await keep the full
//! native-method path, including its argument errors.

use phera_extension_api::{NativeError, NativeMethod, ParamType};
use phera_value::Value;

use crate::{Vm, calls::call_state::CallTarget, classes::MethodBody};

impl Vm {
    pub(crate) fn finish_ready_native_window(&mut self) -> Result<Option<Value>, NativeError> {
        let call = *self.call_stack.top_call()?;
        if !call.is_plain() {
            return Ok(None);
        }
        let Some(target) = self
            .call_stack
            .top_details()
            .and_then(|details| details.callee.as_ref())
            .map(|callee| &callee.target)
        else {
            return Ok(None);
        };
        let CallTarget::Method { method, context } = target else {
            return Ok(None);
        };
        let MethodBody::Native(entry) = &method.body else {
            return Ok(None);
        };
        let NativeMethod::Window(implementation) = entry.handler else {
            return Ok(None);
        };
        if entry.preparation.is_some() || context.invocation != crate::classes::Invocation::Method {
            return Ok(None);
        }
        let argument = match (entry.params, call.sent) {
            ([], 0) => None,
            ([parameter], 1)
                if matches!(parameter.ty, ParamType::Mixed)
                    && !parameter.by_ref
                    && !parameter.variadic =>
            {
                let [Some(value)] = self.registers.window(call.base(), 1) else {
                    return Ok(None);
                };
                Some(value.clone())
            }
            _ => return Ok(None),
        };
        let Some(receiver) = context.receiver.clone() else {
            return Ok(None);
        };
        let Some(result) = (implementation.try_ready)(self, &receiver, argument.as_ref()) else {
            return Ok(None);
        };
        self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        result.map(Some)
    }
}
