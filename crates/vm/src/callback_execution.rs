//! Callback calls retain named arguments and scope: `call_user_func_array($f, ['x' => 1])`.

use std::rc::Rc;

use phera_extension_api::NativeResult;
use phera_value::Value;

use crate::{Vm, call_state::Callee, callback_binding::ArgumentOrigin};

/// The callback protocol carries variadic names alongside its positional parameter slots.
struct CallbackArguments {
    values: Vec<Option<Value>>,
    named_start: usize,
    named_keys: Vec<String>,
}

impl Vm {
    /// PHP execution begins after binding restores the caller's strictness and source mode.
    pub(crate) async fn execute_callback(
        &mut self,
        callee: Rc<Callee>,
        arguments: Vec<Option<Value>>,
        named_start: usize,
        named_keys: Vec<String>,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        self.warn_callback_references(&callee, &arguments);
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        let stop = self.call_stack.depth();
        match &callee.target {
            crate::call_state::CallTarget::Closure(object) => {
                self.with_argument_origin(origin, |vm| {
                    vm.push_closure(object.clone(), None, arguments)
                })?;
                self.name_variadic_arguments(named_start, named_keys)?;
                self.interpret(stop).await
            }
            crate::call_state::CallTarget::User(function) => {
                self.with_argument_origin(origin, |vm| {
                    vm.push_prepared(&function.unit, function.index, None, arguments)
                })?;
                self.name_variadic_arguments(named_start, named_keys)?;
                self.interpret(stop).await
            }
            crate::call_state::CallTarget::Method { method, context } => {
                self.execute_callback_method(
                    &callee.name,
                    method,
                    context,
                    CallbackArguments {
                        values: arguments,
                        named_start,
                        named_keys,
                    },
                    origin,
                )
                .await
            }
            crate::call_state::CallTarget::Native(entry) => {
                self.native_entry_origin(
                    *entry,
                    arguments
                        .into_iter()
                        .map(Option::unwrap_or_default)
                        .collect(),
                    origin,
                )
                .await
            }
            crate::call_state::CallTarget::Intrinsic(intrinsic) => {
                Box::pin(self.intrinsic_origin(
                    *intrinsic,
                    Self::intrinsic_arguments(*intrinsic, arguments),
                    origin,
                ))
                .await
            }
            crate::call_state::CallTarget::Vacant
            | crate::call_state::CallTarget::Constructed(_) => Err(
                phera_extension_api::NativeError::error("Invalid callback target"),
            ),
        }
    }
}

impl Vm {
    /// Method wrappers preserve private access checked when the callable was created.
    async fn execute_callback_method(
        &mut self,
        name: &str,
        method: &crate::classes::Method,
        context: &crate::classes::MethodContext,
        arguments: CallbackArguments,
        origin: ArgumentOrigin,
    ) -> NativeResult {
        let CallbackArguments {
            values: arguments,
            named_start,
            named_keys,
        } = arguments;
        let stop = self.call_stack.depth();
        match &method.body {
            crate::classes::MethodBody::Closure(operation) => {
                self.closure_method_named(
                    *operation,
                    context.receiver.clone(),
                    arguments
                        .into_iter()
                        .map(Option::unwrap_or_default)
                        .collect(),
                    0,
                    Vec::new(),
                    origin,
                )
                .await
            }
            crate::classes::MethodBody::User(function) => {
                self.with_argument_origin(origin, |vm| {
                    vm.push_prepared(&function.unit, function.index, None, arguments)
                })?;
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.method = Some(Box::new(context.clone()));
                }
                self.name_variadic_arguments(named_start, named_keys)?;
                self.interpret(stop).await
            }
            crate::classes::MethodBody::Native(entry) => {
                let mut arguments: Vec<Value> = arguments
                    .into_iter()
                    .map(Option::unwrap_or_default)
                    .collect();
                self.with_argument_origin(origin, |vm| {
                    phera_extension_api::prepare_call(name, entry.params, &mut arguments, vm)
                })?;
                match entry.handler {
                    phera_extension_api::NativeMethod::Sync(handler) => {
                        handler(self, context.receiver.as_ref(), &arguments)
                    }
                    phera_extension_api::NativeMethod::Async(handler) => {
                        handler(self, context.receiver.clone(), arguments).await
                    }
                    phera_extension_api::NativeMethod::Abstract => Err(
                        phera_extension_api::NativeError::error("Cannot call abstract method"),
                    ),
                }
            }
        }
    }
}
