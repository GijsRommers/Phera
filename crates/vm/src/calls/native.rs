//! Native handlers keep the extension API while receiving window arguments.

use crate::Vm;
use crate::calls::call_state::CallTarget;
use crate::calls::call_view::CallView;
use crate::calls::prepared_call::PreparedCallee;
use crate::classes::MethodBody;
use phera_extension_api::{NativeError, NativeFn, NativeResult};
use phera_value::Value;

impl Vm {
    /// Validate native arguments using the calling syntax or internal callback rules.
    pub(crate) async fn native_entry_origin(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        if self.error_handlers.error.is_some() && !self.error_handlers.active {
            self.native_entry_traced(entry, arguments, origin).await
        } else {
            self.native_entry_body(entry, arguments, origin).await
        }
    }

    /// Handler-only state uses an erased future so ordinary native calls retain their small frame.
    fn native_entry_traced(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let previous =
                self.error_handlers
                    .native_site
                    .replace(crate::error_traces::NativeTrace::new(
                        entry.name, &arguments,
                    ));
            let result = self.native_entry_body(entry, arguments, origin).await;
            self.error_handlers.native_site = previous;
            result
        })
    }

    /// Keep warning trace capture outside native execution when no handler is installed.
    async fn native_entry_body(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        mut arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        let previous = std::mem::replace(&mut self.argument_origin, origin);
        let prepared = self.prepare_call_with_trace(entry.name, entry.params, &mut arguments);
        self.argument_origin = previous;
        if let Err(error) = prepared {
            if !matches!(
                entry.name,
                "strlen" | "count" | "sizeof" | "array_key_exists"
            ) {
                self.failed_native = Some(crate::error_traces::NativeTrace::new(
                    entry.name, &arguments,
                ));
            }
            self.call_stack.recycle_native_arguments(arguments);
            return Err(error);
        }
        match entry.handler {
            NativeFn::Sync(handler) => {
                let result = handler(self, &arguments);
                if result.is_err()
                    && !matches!(
                        entry.name,
                        "strlen" | "count" | "sizeof" | "array_key_exists"
                    )
                {
                    self.failed_native = Some(crate::error_traces::NativeTrace::new(
                        entry.name, &arguments,
                    ));
                }
                self.call_stack.recycle_native_arguments(arguments);
                result
            }
            NativeFn::Async(frame) => frame(self, arguments).await,
        }
    }
}

impl Vm {
    /// The extension API owns Vec arguments; only this boundary materializes one.
    pub(crate) async fn finish_native_window(&mut self) -> Result<Value, NativeError> {
        let slots = CallView::top(&self.code, &self.call_stack)?.slots();
        let (call, details) = self.call_stack.pop_call()?;
        if call.callee == PreparedCallee::Constructed {
            let object = self.registers.take(call.base() - 1);
            self.registers.truncate(call.start());
            return Ok(object);
        }
        let details = details.unwrap_or_default();
        let callee = details
            .callee
            .ok_or_else(|| internal_error!("native target"))?;
        let mut arguments = self.call_stack.native_arguments();
        arguments.reserve(call.sent as usize);
        for index in 0..call.sent as usize {
            let value = self.registers.window_mut(slots.slot(index), 1)[0].take();
            let value = if index == 1
                && value.is_none()
                && matches!(
                    callee.target,
                    CallTarget::Intrinsic(crate::intrinsics::Intrinsic::IsCallable)
                ) {
                Value::Bool(false)
            } else {
                value.unwrap_or_default()
            };
            arguments.push(value);
        }
        self.registers.truncate(slots.base);
        let origin = callee.argument_binding.origin(self.argument_origin);
        let named = &details.extra_named;
        match &callee.target {
            CallTarget::Native(entry) => self.native_entry_origin(*entry, arguments, origin).await,
            CallTarget::Intrinsic(intrinsic) => {
                Box::pin(self.intrinsic_origin(*intrinsic, arguments, origin)).await
            }
            CallTarget::Method { method, context } => {
                self.native_method_window(method, context, arguments, named, origin)
                    .await
            }
            _ => Err(internal_error!("native target")),
        }
    }

    /// Native methods preserve constructor results and Closure wrapper argument naming.
    pub(crate) async fn native_method_window(
        &mut self,
        method: &crate::classes::Method,
        context: &crate::classes::MethodContext,
        mut arguments: Vec<Value>,
        named: &[(usize, phera_value::PhpString)],
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> Result<Value, NativeError> {
        let result = match &method.body {
            MethodBody::Enum(operation) => {
                let name = format!("{}::{}", method.declaring, method.name);
                let result = self.with_argument_origin(origin, |vm| {
                    vm.call_enum_method(*operation, &name, &context.scope, &mut arguments)
                });
                if result.is_err() {
                    self.failed_native =
                        Some(crate::error_traces::NativeTrace::new(&name, &arguments));
                }
                self.call_stack.recycle_native_arguments(arguments);
                result
            }
            MethodBody::Closure(operation) => {
                self.closure_window(
                    *operation,
                    context.receiver.clone(),
                    arguments,
                    named,
                    origin,
                )
                .await
            }
            MethodBody::Native(entry) => {
                self.with_argument_origin(origin, |vm| {
                    vm.prepare_native_method_arguments(method, entry.params, &mut arguments)
                })?;
                match entry.handler {
                    phera_extension_api::NativeMethod::Sync(handler) => {
                        let result = handler(self, context.receiver.as_ref(), &arguments);
                        self.call_stack.recycle_native_arguments(arguments);
                        result
                    }
                    phera_extension_api::NativeMethod::Async(handler) => {
                        handler(self, context.receiver.clone(), arguments).await
                    }
                    phera_extension_api::NativeMethod::Abstract => {
                        Err(NativeError::error("Cannot call abstract method"))
                    }
                }
            }
            MethodBody::User(_) => Err(internal_error!("user method in native call")),
        }?;
        if context.invocation == crate::classes::Invocation::Constructor {
            return Ok(context
                .receiver
                .clone()
                .map(Value::Object)
                .unwrap_or_default());
        }
        if matches!(method.declaring.as_str(), "Exception" | "Error") {
            return Ok(context
                .receiver
                .as_ref()
                .and_then(|object| self.throwable_method_result(&method.name, object))
                .unwrap_or(result));
        }
        Ok(result)
    }
    /// Closure wrappers forward named variadic keys to the underlying window protocol.
    fn closure_window(
        &mut self,
        operation: crate::closure_methods::ClosureMethod,
        receiver: Option<std::rc::Rc<phera_value::Object>>,
        arguments: Vec<Value>,
        named: &[(usize, phera_value::PhpString)],
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        let names = named.iter().map(|(_, name)| name.clone()).collect();
        let start = named.first().map_or(0, |(index, _)| *index);
        Box::pin(self.closure_method_named(operation, receiver, arguments, start, names, origin))
    }
}
