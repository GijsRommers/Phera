//! Native methods receive the bound receiver, as in `$pdo->query($sql)`.

use super::callback_binding::ArgumentOrigin;
use super::native_frames::NativeFrame;
use crate::{Vm, classes::MethodBody};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

impl Vm {
    /// Native methods preserve constructor results and Closure wrapper argument naming.
    pub(crate) async fn native_method_window(
        &mut self,
        method: &crate::classes::Method,
        context: &crate::classes::MethodContext,
        mut arguments: Vec<Value>,
        named: &[(usize, phera_value::PhpString)],
        origin: ArgumentOrigin,
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
                if let Err(error) = &result {
                    self.record_native_failure(error, NativeFrame::Method(method), &arguments);
                }
                self.call_stack.recycle_native_arguments(arguments);
                result
            }
            MethodBody::Closure(operation) => {
                let retained = self.retain_native_arguments(&arguments);
                let result = self
                    .closure_window(
                        *operation,
                        context.receiver.clone(),
                        arguments,
                        named,
                        origin,
                    )
                    .await;
                self.finish_native_future(result, NativeFrame::Method(method), retained)
            }
            MethodBody::Native(entry) => {
                if !self.autoload.callbacks.is_empty()
                    && let Err(error) = self
                        .autoload_native_parameters(entry.params, &arguments)
                        .await
                {
                    self.record_native_failure(&error, NativeFrame::Method(method), &arguments);
                    self.call_stack.recycle_native_arguments(arguments);
                    return Err(error);
                }
                let preparation = self.with_argument_origin(origin, |vm| match entry.preparation {
                    Some(prepare) => prepare(vm, entry.params, &mut arguments),
                    None => Ok(phera_extension_api::MethodPreparation::Default),
                });
                let preparation = match preparation {
                    Ok(preparation) => preparation,
                    Err(error) => {
                        self.record_native_failure(&error, NativeFrame::Method(method), &arguments);
                        self.call_stack.recycle_native_arguments(arguments);
                        return Err(error);
                    }
                };
                if matches!(preparation, phera_extension_api::MethodPreparation::Default) {
                    let mut prepared = self.with_argument_origin(origin, |vm| {
                        vm.prepare_native_method_arguments(method, entry.params, &mut arguments)
                    });
                    if prepared.is_err()
                        && self.native_strings_needed(entry.params, &arguments, origin)
                    {
                        let name = method.signature(&self.code).name.clone();
                        self.failed_native = None;
                        if let Err(error) = self
                            .coerce_native_strings(&name, entry.params, &mut arguments, origin)
                            .await
                        {
                            self.record_native_failure(
                                &error,
                                NativeFrame::Method(method),
                                &arguments,
                            );
                            self.call_stack.recycle_native_arguments(arguments);
                            return Err(error);
                        }
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
                // A class miss from the handler loads the class and runs it
                // again with the same arguments (see `calls::native_retries`).
                // They come back with the miss: the synchronous handler only
                // borrowed them, and the others' are the copy kept anyway for
                // the native frame of a failure, so a call that does not miss
                // copies nothing more.
                let mut attempts = 0;
                let result = loop {
                    let (outcome, again) = match entry.handler {
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
                            match result {
                                Err(error) if crate::autoload::ClassMiss::is(&error) => {
                                    (Err(error), Some(std::mem::take(&mut arguments)))
                                }
                                result => {
                                    if let Err(error) = &result {
                                        self.record_native_failure(
                                            error,
                                            NativeFrame::Method(method),
                                            &arguments,
                                        );
                                    }
                                    self.call_stack
                                        .recycle_native_arguments(std::mem::take(&mut arguments));
                                    (result, None)
                                }
                            }
                        }
                        handler => {
                            let retained = self.retain_native_arguments(&arguments);
                            let arguments = std::mem::take(&mut arguments);
                            let result = match handler {
                                phera_extension_api::NativeMethod::Async(handler) => {
                                    handler(self, context.receiver.clone(), arguments).await
                                }
                                phera_extension_api::NativeMethod::Window(window) => {
                                    (window.run)(self, context.receiver.clone(), arguments).await
                                }
                                phera_extension_api::NativeMethod::AsyncNamed(handler) => {
                                    handler(self, context.receiver.clone(), arguments, named).await
                                }
                                _ => Err(NativeError::error("Cannot call abstract method")),
                            };
                            match result {
                                Err(error) if crate::autoload::ClassMiss::is(&error) => {
                                    (Err(error), Some(retained.into_vec()))
                                }
                                result => (
                                    self.finish_native_future(
                                        result,
                                        NativeFrame::Method(method),
                                        retained,
                                    ),
                                    None,
                                ),
                            }
                        }
                    };
                    match (outcome, again) {
                        (Err(error), again) if crate::autoload::ClassMiss::is(&error) => {
                            let retry = again.is_some();
                            match self.load_missed_class(error, &mut attempts, retry).await {
                                Ok(()) => arguments = again.unwrap_or_default(),
                                Err(error) => break Err(error),
                            }
                        }
                        (outcome, _) => break outcome,
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
        origin: ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        let names = named.iter().map(|(_, name)| name.clone()).collect();
        let start = named.first().map_or(0, |(index, _)| *index);
        Box::pin(self.closure_method_named(operation, receiver, arguments, start, names, origin))
    }
}
