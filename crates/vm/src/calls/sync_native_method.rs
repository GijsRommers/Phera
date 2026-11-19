//! Synchronous methods with already typed arguments can stay in the batch.
//!
//! ```php
//! $fields->count();
//! $date->format('Y');
//! ```

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeMethod};
use phera_value::Value;

use super::{call_state::CallTarget, execution_loop::BatchExit, prepared_call::PreparedCall};
use crate::{
    Vm,
    classes::{Invocation, MethodBody},
};

impl Vm {
    #[inline(never)]
    pub(super) fn call_sync_native_method(
        &mut self,
        call: PreparedCall,
        destination: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let fallback = Ok(Some(BatchExit::NativeCall(destination)));
        let Some(details) = self.call_stack.top_details() else {
            return fallback;
        };
        let Some(CallTarget::Method { method, context }) =
            details.callee.as_ref().map(|callee| &callee.target)
        else {
            return fallback;
        };
        let MethodBody::Native(entry) = method.body else {
            return fallback;
        };
        let NativeMethod::Sync(handler) = entry.handler else {
            return fallback;
        };
        let count = call.sent as usize;
        let slots = self.registers.window(call.base(), count);
        if context.invocation == Invocation::Constructor
            || matches!(method.declaring.as_str(), "Exception" | "Error")
            || (self.classes.has_lazy_objects
                && context
                    .receiver
                    .as_ref()
                    .is_some_and(|object| object.lazy().is_some()))
            || entry.preparation.is_some()
            || !call.is_plain()
            || details.named.is_some()
            || count > 4
            || slots.len() != count
            || !super::native_signature::accepts(entry.params, slots)
        {
            return fallback;
        }
        let (_, details) = self.call_stack.pop_call()?;
        let Some(CallTarget::Method { method, context }) = details
            .and_then(|details| details.callee)
            .map(|callee| callee.target)
        else {
            return Err(internal_error!("native method target"));
        };
        let mut arguments = [const { Value::Null }; 4];
        for (offset, argument) in arguments[..count].iter_mut().enumerate() {
            *argument = self.registers.take(call.base() + offset);
        }
        self.registers.truncate(call.start());
        let previous = self.native_called_class.take();
        if method.is_static {
            self.native_called_class = Some(context.called);
        }
        let pending = self.error_handlers.pending.len();
        let result = handler(self, context.receiver.as_ref(), &arguments[..count]);
        self.native_called_class = previous;
        if pending != self.error_handlers.pending.len() {
            let operator = if method.is_static { "::" } else { "->" };
            let name = format!("{}{operator}{}", method.declaring, method.name);
            self.attach_native_diagnostics(pending, &name, &arguments[..count]);
        }
        match result {
            Ok(value) => {
                self.set(destination, value);
                Ok(None)
            }
            Err(error)
                if Self::is_reflected_hook(&error)
                    || crate::lazy_continuation::ReflectInitialize::is_pending(&error) =>
            {
                Ok(Some(BatchExit::NativeMethodHook(
                    destination,
                    Box::new((error, method)),
                )))
            }
            Err(error) => {
                if matches!(&error, NativeError::Throw { .. }) {
                    self.failed_native = Some(crate::error_traces::NativeTrace::method(
                        &method,
                        &arguments[..count],
                    ));
                }
                Err(error)
            }
        }
    }
}
