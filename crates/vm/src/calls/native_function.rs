//! `sprintf('%s')` raises an error from a native body after binding its arguments.

use super::callback_binding::{ArgumentBinding, ArgumentOrigin};
use super::native_frames::NativeFrame;
use crate::Vm;
use phera_extension_api::{NativeError, NativeFn, NativeResult};
use phera_value::Value;

impl Vm {
    /// Validate native arguments using the calling syntax or internal callback rules.
    pub(crate) async fn native_entry_origin(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
        binding: ArgumentBinding,
    ) -> NativeResult {
        if binding.has_native_frame()
            && self.error_handlers.error.is_some()
            && !self.error_handlers.active
        {
            self.native_entry_traced(entry, arguments, origin, binding)
                .await
        } else {
            self.native_entry_body(entry, arguments, origin, binding)
                .await
        }
    }

    /// Handler-only state uses an erased future so ordinary native calls retain their small frame.
    fn native_entry_traced(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: Vec<Value>,
        origin: ArgumentOrigin,
        binding: ArgumentBinding,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let trace = self.native_parameter_trace(entry.name, entry.params, &arguments);
            let previous = self.error_handlers.native_site.replace(trace);
            let result = self
                .native_entry_body(entry, arguments, origin, binding)
                .await;
            self.error_handlers.native_site = previous;
            result
        })
    }

    /// Keep warning trace capture outside native execution when no handler is installed.
    async fn native_entry_body(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        mut arguments: Vec<Value>,
        origin: ArgumentOrigin,
        binding: ArgumentBinding,
    ) -> NativeResult {
        if matches!(entry.handler, NativeFn::DeprecatedSync(_))
            && let Some(message) = phera_source::deprecations::kept_notice(
                phera_source::deprecations::ItemKind::Function,
                entry.name,
            )
        {
            self.diagnostic("Deprecated", message.into());
            self.drain_error_handlers().await?;
        }
        // Asked here, not inside the call: without a registered loader a
        // native call must not even build the future that would look.
        if !self.autoload.callbacks.is_empty()
            && let Err(error) = self
                .autoload_native_parameters(entry.params, &arguments)
                .await
        {
            if binding.has_native_frame() {
                self.record_native_failure(&error, NativeFrame::Function(entry.name), &arguments);
            }
            self.call_stack.recycle_native_arguments(arguments);
            return Err(error);
        }
        let previous = std::mem::replace(&mut self.argument_origin, origin);
        let prepared =
            self.prepare_call_recording(entry.name, entry.params, &mut arguments, binding);
        self.argument_origin = previous;
        if let Err(error) = prepared
            && let Err(error) = self
                .retry_native_strings(entry, &mut arguments, origin, binding, error)
                .await
        {
            if binding.has_native_frame() {
                self.record_native_failure(&error, NativeFrame::Function(entry.name), &arguments);
            }
            self.call_stack.recycle_native_arguments(arguments);
            return Err(error);
        }
        if matches!(arguments.first(), Some(Value::Object(_)))
            && let Some(handler) = self.registry.object_handler(entry.name)
        {
            return self
                .run_async_native_function(entry.name, handler, arguments, binding)
                .await;
        }
        match entry.handler {
            NativeFn::Sync(handler) | NativeFn::DeprecatedSync(handler) => {
                let result = handler(self, &arguments);
                // Kept for a second call (see `calls::native_retries`).
                if let Err(error) = &result
                    && crate::autoload::ClassMiss::is(error)
                {
                    self.keep_missed_arguments(&arguments);
                }
                if let Err(error) = &result
                    && binding.has_native_frame()
                {
                    self.record_native_failure(
                        error,
                        NativeFrame::Function(entry.name),
                        &arguments,
                    );
                }
                self.call_stack.recycle_native_arguments(arguments);
                result
            }
            NativeFn::Async(handler) => {
                self.run_async_native_function(entry.name, handler, arguments, binding)
                    .await
            }
        }
    }

    /// The successful native call keeps the synchronous argument parser. Only
    /// a failed parse can request an interpreter call for a string conversion.
    #[cold]
    fn retry_native_strings<'a>(
        &'a mut self,
        entry: phera_extension_api::FunctionEntry,
        arguments: &'a mut [Value],
        origin: ArgumentOrigin,
        binding: ArgumentBinding,
        original_error: NativeError,
    ) -> phera_extension_api::LocalFuture<'a, Result<(), NativeError>> {
        Box::pin(async move {
            if !self.native_strings_needed(entry.params, arguments, origin) {
                return Err(original_error);
            }
            // A conversion can enter PHP and suspend before the native body runs.
            let traced = binding.has_native_frame();
            if traced {
                let trace = self.native_parameter_trace(entry.name, entry.params, arguments);
                self.native_callback_sites
                    .push(crate::error_traces::NativeCallbackSite {
                        boundary: self.call_stack.depth(),
                        native: crate::error_traces::NativeCaller::Saved(trace),
                    });
            }
            let converted = self
                .coerce_native_strings(entry.name, entry.params, arguments, origin)
                .await;
            if traced {
                self.native_callback_sites.pop();
            }
            converted?;
            self.with_argument_origin(origin, |vm| {
                vm.prepare_call_recording(entry.name, entry.params, arguments, binding)
            })
        })
    }

    async fn run_async_native_function(
        &mut self,
        name: &'static str,
        handler: phera_extension_api::AsyncFn,
        arguments: Vec<Value>,
        binding: ArgumentBinding,
    ) -> NativeResult {
        if !binding.has_native_frame() {
            return handler(self, arguments).await;
        }
        if self.registry.has_callback_frame(name) {
            return self.native_callback_frame(name, handler, arguments).await;
        }
        let retained = self.retain_native_arguments(&arguments);
        let result = handler(self, arguments).await;
        // Kept for a second call (see `calls::native_retries`).
        if let Err(error) = &result
            && crate::autoload::ClassMiss::is(error)
        {
            self.keep_missed_arguments(retained.as_slice());
        }
        self.finish_native_future(result, NativeFrame::Function(name), retained)
    }

    /// Publish the native frame while PHP callbacks are active, for Throwable::getTrace().
    fn native_callback_frame(
        &mut self,
        name: &'static str,
        handler: phera_extension_api::AsyncFn,
        arguments: Vec<Value>,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        let native = self.native_function_trace(name, &arguments);
        Box::pin(async move {
            self.native_callback_sites
                .push(crate::error_traces::NativeCallbackSite {
                    boundary: self.call_stack.frames().len(),
                    native: crate::error_traces::NativeCaller::Saved(native),
                });
            let result = handler(self, arguments).await;
            let site = self.native_callback_sites.pop();
            // Kept for a second call (see `calls::native_retries`).
            if let Err(error) = &result
                && crate::autoload::ClassMiss::is(error)
            {
                let trace = site.as_ref().and_then(|site| match &site.native {
                    crate::error_traces::NativeCaller::Saved(trace) => Some(trace),
                    _ => None,
                });
                self.keep_traced_arguments(name, trace);
            }
            if let Err(error) = &result
                && self.needs_native_failure_frame(error)
            {
                self.failed_native = site.and_then(|site| match site.native {
                    crate::error_traces::NativeCaller::Saved(trace) => Some(trace),
                    _ => None,
                });
            }
            result
        })
    }
}
