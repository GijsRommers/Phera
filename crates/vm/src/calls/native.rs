//! Native handlers keep the extension API while receiving window arguments.

use super::native_frames::NativeFrame;
use crate::Vm;
use crate::calls::call_state::CallTarget;
use crate::calls::call_view::CallView;
use crate::calls::prepared_call::PreparedCallee;
use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeFn};
use phera_value::Value;

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
        destination: Option<Reg>,
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
                .native_entry_origin(entry, arguments, origin, callee.argument_binding)
                .await
                .map(Some);
        }
        let mut arguments = self.call_stack.native_arguments();
        arguments.reserve(call.sent as usize);
        let mut missing = None;
        for index in 0..call.sent as usize {
            let value = self.registers.window_mut(slots.slot(index), 1)[0].take();
            let value = value.unwrap_or_else(|| match self.fill_native_gap(&callee, index) {
                Ok(value) => value,
                Err(error) => {
                    missing.get_or_insert(error);
                    Value::Null
                }
            });
            arguments.push(value);
        }
        self.registers.truncate(slots.base);
        if let Some(error) = missing {
            self.record_native_gap_error(&callee, &error, &arguments);
            self.call_stack.recycle_native_arguments(arguments);
            return Err(error);
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
                let mut result = self
                    .native_entry_origin(*entry, arguments, origin, callee.argument_binding)
                    .await;
                // A class miss loads the class and calls the native again
                // (see `calls::native_retries`).
                if let Err(error) = &result
                    && crate::autoload::ClassMiss::is(error)
                    && let Err(error) = result
                {
                    result = self
                        .retry_missed_function(*entry, (origin, callee.argument_binding), error)
                        .await;
                }
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
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Snapshot) => {
                self.snapshot_intrinsic(destination, arguments).map(Some)
            }
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Clone)
                if let Some(destination) = destination =>
            {
                Box::pin(self.suspend_clone_intrinsic(destination, arguments)).await?;
                Ok(None)
            }
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Class(
                crate::class_function::ClassFunction::PropertyExists,
            )) if let (Some(destination), [Value::Object(object), Value::String(name)]) =
                (destination, arguments.as_slice())
                && object.lazy().is_some() =>
            {
                Box::pin(self.lazy_property_exists(destination, object.clone(), name.clone()))
                    .await?;
                self.call_stack.recycle_native_arguments(arguments);
                Ok(None)
            }
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Class(
                crate::class_function::ClassFunction::GetObjectVars,
            )) if let (Some(destination), [Value::Object(object)]) =
                (destination, arguments.as_slice())
                && object.lazy().is_some() =>
            {
                Box::pin(self.lazy_object_vars(destination, object.clone())).await?;
                self.call_stack.recycle_native_arguments(arguments);
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
                if matches!(intrinsic, crate::intrinsics::Intrinsic::CallUserFunc)
                    && !named.is_empty()
                {
                    let retained = self.retain_native_arguments(&arguments);
                    let result = Box::pin(self.invoke_named_user_callback(arguments, named)).await;
                    return self
                        .finish_native_future(result, NativeFrame::Function(&callee.name), retained)
                        .map(Some);
                }
                // Boxed: an intrinsic such as `eval` runs code, and its future
                // must not make the future of every ordinary native call larger.
                if !callee.argument_binding.has_native_frame()
                    || matches!(
                        intrinsic,
                        crate::intrinsics::Intrinsic::ArrayMap
                            | crate::intrinsics::Intrinsic::ParentProperty(_)
                    )
                {
                    return Box::pin(self.intrinsic_origin(*intrinsic, arguments, origin))
                        .await
                        .map(Some);
                }
                let retained = self.retain_native_arguments(&arguments);
                let result = Box::pin(self.intrinsic_origin(*intrinsic, arguments, origin)).await;
                self.finish_native_future(result, NativeFrame::Function(&callee.name), retained)
                    .map(Some)
            }
            CallTarget::Method { method, context } => {
                let mut result = self
                    .native_method_window(method, context, arguments, named, origin)
                    .await;
                // A class miss loads the class and calls the native again
                // (see `calls::native_retries`).
                if let Err(error) = &result
                    && crate::autoload::ClassMiss::is(error)
                    && let Err(error) = result
                {
                    result = self
                        .retry_missed_method((method, context), (named, origin), error)
                        .await;
                }
                match result {
                    Ok(value) => Ok(Some(value)),
                    Err(error) => {
                        self.finish_native_method_continuation(error, method, destination)
                            .await
                    }
                }
            }
            _ => Err(internal_error!("native target")),
        }
    }

    /// A native reflection access can enter a lazy initializer or a property hook.
    #[cold]
    pub(crate) async fn finish_native_method_continuation(
        &mut self,
        error: NativeError,
        method: &crate::classes::Method,
        destination: Option<Reg>,
    ) -> Result<Option<Value>, NativeError> {
        match crate::lazy_continuation::ReflectInitialize::take(error) {
            Ok(request) => Box::pin(self.finish_reflected_lazy(request, method, destination)).await,
            Err(error) => self.finish_reflected_hook(error, method, destination).await,
        }
    }
}
