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
        // Asked here, not inside the call: without a registered loader a
        // native call must not even build the future that would look.
        if !self.autoload.callbacks.is_empty() {
            self.autoload_native_parameters(entry.params, &arguments)
                .await?;
        }
        let previous = std::mem::replace(&mut self.argument_origin, origin);
        let prepared = self.prepare_call_with_trace(entry.name, entry.params, &mut arguments);
        self.argument_origin = previous;
        if let Err(error) = prepared
            && let Err(error) = self
                .retry_native_strings(entry, &mut arguments, origin, error)
                .await
        {
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
        if matches!(arguments.first(), Some(Value::Object(_)))
            && let Some(handler) = self.registry.object_handler(entry.name)
        {
            return handler(self, arguments).await;
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
            NativeFn::Async(handler) if self.registry.has_callback_frame(entry.name) => {
                self.native_callback_frame(entry.name, handler, arguments)
                    .await
            }
            NativeFn::Async(handler) => handler(self, arguments).await,
        }
    }

    /// The successful native call keeps the synchronous argument parser. Only
    /// a failed parse can request an interpreter call for a string conversion.
    #[cold]
    fn retry_native_strings<'a>(
        &'a mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: &'a mut [Value],
        origin: crate::calls::callback_binding::ArgumentOrigin,
        original_error: NativeError,
    ) -> phera_extension_api::LocalFuture<'a, Result<(), NativeError>> {
        Box::pin(async move {
            if !self.native_strings_needed(entry.params, arguments, origin) {
                return Err(original_error);
            }
            self.coerce_native_strings(entry.name, entry.params, arguments, origin)
                .await?;
            self.with_argument_origin(origin, |vm| {
                vm.prepare_call_with_trace(entry.name, entry.params, arguments)
            })
        })
    }

    /// Publish the native frame while PHP callbacks are active, for Throwable::getTrace().
    fn native_callback_frame(
        &mut self,
        name: &str,
        handler: phera_extension_api::AsyncFn,
        arguments: Vec<Value>,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        let native = crate::error_traces::NativeTrace::new(name, &arguments);
        Box::pin(async move {
            self.native_callback_sites
                .push(crate::error_traces::NativeCallbackSite {
                    boundary: self.call_stack.frames().len(),
                    native,
                });
            let result = handler(self, arguments).await;
            let site = self.native_callback_sites.pop();
            if let Err(error) = &result
                && !matches!(error, NativeError::Unwind(_))
                && self.failed_native.is_none()
            {
                self.failed_native = site.map(|site| site.native);
            }
            result
        })
    }
}

impl Vm {
    /// The extension API owns Vec arguments; only this boundary materializes one.
    pub(crate) async fn finish_native_window(&mut self) -> Result<Value, NativeError> {
        self.finish_native_window_at(None)
            .await?
            .ok_or_else(|| internal_error!("native result destination"))
    }

    /// An intrinsic with a PHP caller can retain its destination on a suspended hook.
    pub(crate) async fn finish_native_window_at(
        &mut self,
        destination: Option<phera_bytecode::Reg>,
    ) -> Result<Option<Value>, NativeError> {
        let slots = CallView::top(&self.code, &self.call_stack)?.slots();
        let (call, details) = self.call_stack.pop_call()?;
        if call.callee == PreparedCallee::Constructed {
            let object = self.registers.take(call.base() - 1);
            self.registers.truncate(call.start());
            return Ok(Some(object));
        }
        let details = details.unwrap_or_default();
        let callee = details
            .callee
            .ok_or_else(|| internal_error!("native target"))?;
        if !details.extra_named.is_empty()
            && let CallTarget::Native(mut entry) = callee.target
            && let Some(handler) = self.registry.extra_named_handler(entry.name)
        {
            let mut arguments = self.call_stack.native_arguments();
            let mut extra = details.extra_named.iter().map(|(slot, _)| *slot).peekable();
            for index in 0..call.sent as usize {
                if extra.peek() == Some(&index) {
                    extra.next();
                    continue;
                }
                if let Some(value) = self.registers.window_mut(slots.slot(index), 1)[0].take() {
                    arguments.push(value);
                }
            }
            self.registers.truncate(slots.base);
            entry.handler = NativeFn::Sync(handler);
            let origin = callee.argument_binding.origin(self.argument_origin);
            return self
                .native_entry_origin(entry, arguments, origin)
                .await
                .map(Some);
        }
        let mut arguments = self.call_stack.native_arguments();
        arguments.reserve(call.sent as usize);
        let mut missing = None;
        for index in 0..call.sent as usize {
            let value = self.registers.window_mut(slots.slot(index), 1)[0].take();
            let value = value.unwrap_or_else(|| match callee.target {
                CallTarget::Intrinsic(intrinsic) => intrinsic.named_default(index),
                CallTarget::Native(entry) => {
                    if let Some(parameter) = entry.params.get(index)
                        && !parameter.optional
                    {
                        missing.get_or_insert((entry.name, index + 1, parameter.name));
                    }
                    named_default(entry.params, index)
                }
                CallTarget::Method { ref method, .. } => match method.body {
                    MethodBody::Native(entry) => named_default(entry.params, index),
                    _ => Value::Null,
                },
                _ => Value::Null,
            });
            arguments.push(value);
        }
        self.registers.truncate(slots.base);
        if let Some((function, number, parameter)) = missing {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(function, &arguments));
            self.call_stack.recycle_native_arguments(arguments);
            return Err(NativeError::argument_count_error(format!(
                "{function}(): Argument #{number} (${parameter}) not passed"
            )));
        }
        let origin = callee.argument_binding.origin(self.argument_origin);
        let named = &details.extra_named;
        match &callee.target {
            CallTarget::Native(entry) => {
                let previous = self.native_called_class.take();
                let previous_view = std::mem::replace(
                    &mut self.native_view_depth,
                    destination.map(|_| self.call_stack.depth()),
                );
                let result = self.native_entry_origin(*entry, arguments, origin).await;
                self.native_view_depth = previous_view;
                self.native_called_class = previous;
                if self.output_writer.buffers.pending.is_some() && result.is_ok() {
                    self.output_writer.chunk_caller = Some(entry.name);
                }
                match result {
                    Ok(value) => Ok(Some(value)),
                    Err(error) if let Some(destination) = destination => {
                        Box::pin(self.complete_native_view_result(destination, Err(error))).await
                    }
                    Err(error) => Err(error),
                }
            }
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Clone)
                if let Some(destination) = destination =>
            {
                Box::pin(self.suspend_clone_intrinsic(destination, arguments)).await?;
                Ok(None)
            }
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Class(
                crate::class_function::ClassFunction::GetObjectVars,
            )) if let (Some(destination), [Value::Object(object)]) =
                (destination, arguments.as_slice())
                && let Some(class) = self.hooked_property_class(object) =>
            {
                Box::pin(self.suspend_object_vars(object.clone(), class, destination)).await?;
                self.call_stack.recycle_native_arguments(arguments);
                Ok(None)
            }
            CallTarget::Intrinsic(intrinsic) => {
                // Boxed: an intrinsic such as `eval` runs code, and its future
                // must not make the future of every ordinary native call larger.
                Box::pin(self.intrinsic_origin(*intrinsic, arguments, origin))
                    .await
                    .map(Some)
            }
            CallTarget::Method { method, context } => {
                let result = self
                    .native_method_window(method, context, arguments, named, origin)
                    .await;
                match result {
                    Ok(value) => Ok(Some(value)),
                    Err(error) => match crate::lazy_continuation::ReflectInitialize::take(error) {
                        Ok(request) => {
                            Box::pin(self.finish_reflected_lazy(request, destination)).await
                        }
                        Err(error) => self.finish_reflected_hook(error, method, destination).await,
                    },
                }
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
                let result = loop {
                    let result = self.with_argument_origin(origin, |vm| {
                        vm.call_enum_method(*operation, &name, &context.scope, &mut arguments)
                    });
                    let Err(error) = result else {
                        break result;
                    };
                    let miss = match crate::autoload::ClassMiss::take(error) {
                        Ok(miss) => miss,
                        Err(error) => break Err(error),
                    };
                    match self
                        .lookup_class_from(&miss.name, Some((&name, &arguments)))
                        .await
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => break Err(miss.error()),
                        Err(error) => break Err(error),
                    }
                };
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
                if !self.autoload.callbacks.is_empty() {
                    self.autoload_native_parameters(entry.params, &arguments)
                        .await?;
                }
                let preparation =
                    self.with_argument_origin(origin, |vm| match entry.preparation {
                        Some(prepare) => prepare(vm, entry.params, &mut arguments),
                        None => Ok(phera_extension_api::MethodPreparation::Default),
                    })?;
                if matches!(preparation, phera_extension_api::MethodPreparation::Default) {
                    let mut prepared = self.with_argument_origin(origin, |vm| {
                        vm.prepare_native_method_arguments(method, entry.params, &mut arguments)
                    });
                    if prepared.is_err()
                        && self.native_strings_needed(entry.params, &arguments, origin)
                    {
                        let name = method.signature(&self.code).name.clone();
                        self.failed_native = None;
                        self.coerce_native_strings(&name, entry.params, &mut arguments, origin)
                            .await?;
                        prepared = self.with_argument_origin(origin, |vm| {
                            vm.prepare_native_method_arguments(method, entry.params, &mut arguments)
                        });
                    }
                    prepared?;
                }
                if let phera_extension_api::MethodPreparation::Return(value) = preparation {
                    self.call_stack.recycle_native_arguments(arguments);
                    return Ok(value);
                }
                let previous = self.native_called_class.take();
                if method.is_static {
                    self.native_called_class = Some(context.called.clone());
                }
                let result = match entry.handler {
                    phera_extension_api::NativeMethod::Sync(handler) => {
                        let track =
                            self.error_handlers.error.is_some() && !self.error_handlers.active;
                        let previous = if track {
                            let operator = if method.is_static { "::" } else { "->" };
                            self.error_handlers.native_site.replace(
                                crate::error_traces::NativeTrace::new(
                                    &format!("{}{operator}{}", method.declaring, method.name),
                                    &arguments,
                                ),
                            )
                        } else {
                            None
                        };
                        let result = handler(self, context.receiver.as_ref(), &arguments);
                        if track {
                            self.error_handlers.native_site = previous;
                        }
                        self.call_stack.recycle_native_arguments(arguments);
                        result
                    }
                    phera_extension_api::NativeMethod::Async(handler) => {
                        handler(self, context.receiver.clone(), arguments).await
                    }
                    phera_extension_api::NativeMethod::Window(method) => {
                        (method.run)(self, context.receiver.clone(), arguments).await
                    }
                    phera_extension_api::NativeMethod::AsyncNamed(handler) => {
                        handler(self, context.receiver.clone(), arguments, named).await
                    }
                    phera_extension_api::NativeMethod::Abstract => {
                        Err(NativeError::error("Cannot call abstract method"))
                    }
                };
                self.native_called_class = previous;
                result
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

// Omitted optional tail arguments still use the handler's usual defaults.
fn named_default(parameters: &[phera_extension_api::Param], index: usize) -> Value {
    parameters
        .get(index)
        .and_then(|parameter| parameter.default)
        .map(|default| default.to_value())
        .unwrap_or_default()
}
