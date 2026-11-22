//! Synchronous methods with already typed arguments can stay in the batch.
//!
//! ```php
//! $fields->count();
//! $date->format('Y');
//! ```

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeMethod};
use phera_value::Value;

use super::{
    call_state::{CallTarget, Callee},
    execution_loop::BatchExit,
    prepared_call::{CallDetails, PreparedCall},
};
use crate::{
    Vm,
    calls::native_frames::NativeFrame,
    classes::{Invocation, MethodBody, MethodContext},
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
        if let Some(ready) = entry.ready {
            return self.call_ready_native_method(call, destination, ready);
        }
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
        let Some(CallDetails {
            callee:
                Some(Callee {
                    argument_binding,
                    name,
                    parameters,
                    target: CallTarget::Method { method, context },
                }),
            named,
            extra_named,
        }) = details
        else {
            return Err(internal_error!("native method target"));
        };
        let mut arguments = [const { Value::Null }; 4];
        for (offset, argument) in arguments[..count].iter_mut().enumerate() {
            *argument = self.registers.take(call.base() + offset);
        }
        self.registers.truncate(call.start());
        let MethodContext {
            receiver,
            scope,
            called,
            invocation,
        } = context;
        let mut called = Some(called);
        let previous = self.native_called_class.take();
        if method.is_static {
            self.native_called_class = called.take();
        }
        let pending = self.error_handlers.pending.len();
        let result = handler(self, receiver.as_ref(), &arguments[..count]);
        let lent = std::mem::replace(&mut self.native_called_class, previous);
        if pending != self.error_handlers.pending.len() {
            let operator = if method.is_static { "::" } else { "->" };
            let name = format!("{}{operator}{}", method.declaring, method.name);
            self.attach_native_diagnostics(pending, &name, &arguments[..count]);
        }
        match result {
            Ok(value) => {
                self.set_call_result(destination, value);
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
            // The interpreter runs the `Call` again once the class is loaded
            // (see `calls::native_retries`).
            Err(error) if crate::autoload::ClassMiss::is(&error) => {
                let taken = TakenMethodCall {
                    argument_binding,
                    name,
                    parameters,
                    method,
                    receiver,
                    scope,
                    called: called.or(lent),
                    invocation,
                    named,
                    extra_named,
                };
                Err(self.restore_native_method_call(call, taken, &arguments[..count], error))
            }
            Err(error) => {
                self.record_native_failure(
                    &error,
                    NativeFrame::Method(&method),
                    &arguments[..count],
                );
                Err(error)
            }
        }
    }

    /// Finish a typed call such as `new ReflectionClass(Foo::class)`.
    /// On a miss, leave its prepared call and arguments for the ordinary binder.
    #[inline(never)]
    fn call_ready_native_method(
        &mut self,
        call: PreparedCall,
        destination: Reg,
        ready: phera_extension_api::ReadyMethod,
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
        let count = call.sent as usize;
        let slots = self.registers.window(call.base(), count);
        let Some(receiver) = context.receiver.as_ref() else {
            return fallback;
        };
        if !call.is_plain()
            || details.named.is_some()
            || !details.extra_named.is_empty()
            || (self.classes.has_lazy_objects && receiver.lazy().is_some())
            || count > 2
            || slots.len() != count
            || !super::native_signature::accepts_ready(entry.params, slots)
        {
            return fallback;
        }
        let mut arguments = [const { Value::Null }; 2];
        let receiver = receiver.clone();
        let construction = context.invocation == Invocation::Constructor;
        for (offset, argument) in arguments[..count].iter_mut().enumerate() {
            *argument = self.registers.take(call.base() + offset);
        }
        let Some(result) = ready(self, &receiver, &arguments[..count]) else {
            for (offset, argument) in arguments.into_iter().take(count).enumerate() {
                self.registers.put(call.base() + offset, argument);
            }
            return fallback;
        };
        let (_, details) = self.call_stack.pop_call()?;
        self.registers.truncate(call.start());
        match result {
            Ok(value) => {
                self.set(
                    destination,
                    if construction {
                        Value::Object(receiver)
                    } else {
                        value
                    },
                );
                Ok(None)
            }
            Err(error) => {
                if let Some(CallTarget::Method { method, .. }) = details
                    .and_then(|details| details.callee)
                    .map(|callee| callee.target)
                {
                    self.record_native_failure(
                        &error,
                        NativeFrame::Method(&method),
                        &arguments[..count],
                    );
                }
                Err(error)
            }
        }
    }

    /// Puts a fast native method call back as it was after its handler
    /// missed a class, so that the interpreter's retry of the `Call` finds
    /// it: the record rebuilt from the parts the call took apart, `$this`
    /// below the window, the arguments in it. Gives the error to unwind
    /// with.
    #[cold]
    #[inline(never)]
    fn restore_native_method_call(
        &mut self,
        call: PreparedCall,
        taken: TakenMethodCall,
        arguments: &[Value],
        error: NativeError,
    ) -> NativeError {
        let TakenMethodCall {
            argument_binding,
            name,
            parameters,
            method,
            receiver,
            scope,
            called,
            invocation,
            named,
            extra_named,
        } = taken;
        // A native method's receiver travels in its context; a window that
        // also keeps `$this` below its first register gets it back there.
        // Another shape cannot be rebuilt here, and reports the missing class.
        let (Some(called), slots @ (0 | 1)) = (called, call.kind.prefix()) else {
            return crate::autoload::ClassMiss::take(error)
                .map_or_else(|error| error, crate::autoload::ClassMiss::error);
        };
        let this = receiver.clone().filter(|_| slots == 1).map(Value::Object);
        let details = CallDetails {
            callee: Some(Callee {
                argument_binding,
                name,
                parameters,
                target: CallTarget::Method {
                    method,
                    context: MethodContext {
                        receiver,
                        scope,
                        called,
                        invocation,
                    },
                },
            }),
            named,
            extra_named,
        };
        self.restore_native_call((call, Some(details)), [this, None], arguments);
        error
    }
}

/// The parts a fast native method call takes its record apart into, kept
/// for a class miss to put the record back (see
/// [`Vm::restore_native_method_call`]).
struct TakenMethodCall {
    argument_binding: super::callback_binding::ArgumentBinding,
    name: std::rc::Rc<str>,
    parameters: std::rc::Rc<[super::call_state::ParameterSpec]>,
    method: std::rc::Rc<crate::classes::Method>,
    receiver: Option<std::rc::Rc<phera_value::Object>>,
    scope: std::rc::Rc<crate::classes::Class>,
    /// `None` while a static method's class is lent to the handler.
    called: Option<std::rc::Rc<crate::classes::Class>>,
    invocation: Invocation,
    named: Option<phera_value::PhpString>,
    extra_named: Vec<(usize, phera_value::PhpString)>,
}
