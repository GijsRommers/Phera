//! Native callbacks fill ordinary call windows and run to a heap-frame boundary.

use phera_extension_api::NativeResult;
use phera_value::Value;

use crate::Vm;
use crate::calls::call_state::Callee;
use crate::calls::callback_binding::ArgumentOrigin;

impl Vm {
    /// Consume native API values directly, without building a second argument vector.
    pub(crate) fn invoke_callable_origin(
        &mut self,
        callee: Callee,
        arguments: impl IntoIterator<Item = Value>,
        origin: ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        let origin = callee.argument_binding.origin(origin);
        let base = self.registers.len();
        let depth = self.call_stack.call_depth();
        let routed = self.prepare_call(callee).and_then(|()| {
            for (index, value) in arguments.into_iter().enumerate() {
                self.store_sent_value(index, value)?;
            }
            Ok(())
        });
        self.finish_callback(routed, base, depth, origin)
    }

    /// Every callback boundary restores unfinished outer calls, even when entry fails.
    pub(crate) fn finish_callback(
        &mut self,
        routed: Result<(), phera_extension_api::NativeError>,
        base: usize,
        depth: usize,
        origin: ArgumentOrigin,
    ) -> phera_extension_api::LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            let entered = self.call_stack.enter_callback();
            let result = match entered {
                Ok(()) => {
                    let result = match routed {
                        Ok(()) => self.run_callback_window(origin).await,
                        Err(error) => Err(error),
                    };
                    self.call_stack.leave_callback();
                    result
                }
                Err(error) => Err(error),
            };
            let result = result.map_err(|error| self.materialize_error(error));
            self.call_stack.truncate_calls(depth);
            self.registers.truncate(base);
            result
        })
    }

    /// Restore argument strictness before interpreting the PHP body, even when entry fails.
    pub(crate) async fn run_callback_window(&mut self, origin: ArgumentOrigin) -> NativeResult {
        self.warn_callback_references();
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        let stop = self.call_stack.depth();
        let entered = self.with_argument_origin(origin, |vm| vm.enter_prepared_call(None));
        match entered {
            Ok(true) => self.interpret(stop).await,
            Ok(false) => {
                let previous = std::mem::replace(&mut self.argument_origin, origin);
                let result = Box::pin(self.finish_native_window()).await;
                self.argument_origin = previous;
                result
            }
            Err(error) => {
                self.handle_exception(error, stop)?;
                Err(internal_error!("callback error escaped boundary"))
            }
        }
    }
}
