//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::Target;
use phera_extension_api::{LocalFuture, NativeError, NativeResult};

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use crate::dispatch::DispatchResult;

impl Vm {
    /// Runs frames until the call stack is back at depth `stop`.
    ///
    /// The inner loops return an error for anything that goes wrong. This outer
    /// loop turns that error into a PHP exception and looks for a `catch` or
    /// `finally` (`handle_exception`). If one is found, execution continues in
    /// that frame; if not, the error leaves the interpreter.
    ///
    /// The future is boxed because the interpreter is re-entered recursively
    /// (a native function calling back into PHP), and a recursive `async fn`
    /// needs a known size.
    pub(crate) fn interpret(&mut self, stop: usize) -> LocalFuture<'_, NativeResult> {
        self.continue_execution(stop, 0, None)
    }

    /// A caller that already ran a synchronous batch can hand its suspension to the async shell.
    pub(crate) fn continue_execution(
        &mut self,
        stop: usize,
        mut work: usize,
        mut pending: Option<Result<BatchExit, NativeError>>,
    ) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            loop {
                match self.interpret_frames(stop, &mut work, pending.take()).await {
                    Ok(value) => return Ok(value),
                    Err(error) => {
                        let error = match crate::autoload::ClassMiss::take(error) {
                            Ok(miss) => match Box::pin(self.resume_class_fetch(miss)).await {
                                Ok(()) => continue,
                                Err(error) => error,
                            },
                            Err(error) => error,
                        };
                        let error = if self.error_handlers.pending.is_empty() {
                            error
                        } else {
                            match self.drain_error_handlers().await {
                                Ok(()) => error,
                                Err(handler_error) => handler_error,
                            }
                        };
                        self.handle_exception(error, stop)?;
                    }
                }
            }
        })
    }
}

impl Vm {
    /// Await cold operations while the synchronous loop owns ordinary calls and scalar work.
    async fn interpret_frames(
        &mut self,
        stop: usize,
        work: &mut usize,
        mut pending: Option<Result<BatchExit, NativeError>>,
    ) -> NativeResult {
        loop {
            let exit = match pending.take() {
                Some(exit) => exit?,
                None => self.run_batch(stop, work)?,
            };
            match exit {
                BatchExit::Returned(value) => return Ok(value),
                BatchExit::Yield => tokio::task::yield_now().await,
                BatchExit::Handlers => {}
                BatchExit::Compare(pending) => {
                    let order = self.finish_string_comparison(pending.cast).await?;
                    let value = crate::string_comparison::comparison_value(pending.operator, order);
                    self.set(pending.destination, value);
                }
                BatchExit::BindCall(destination) => {
                    self.coerce_call_strings(self.argument_origin).await?;
                    if !matches!(
                        self.enter_prepared_call(Some(destination))?,
                        crate::calls::call_execution::CallEntry::Php
                    ) && let Some(value) =
                        self.finish_native_window_at(Some(destination)).await?
                    {
                        self.set(destination, value);
                    }
                }
                BatchExit::NativeCall(destination) => {
                    if let Some(value) = self.finish_native_window_at(Some(destination)).await? {
                        self.set(destination, value);
                    }
                }
                BatchExit::Instruction(instruction) => {
                    if let DispatchResult::Returned(value) =
                        Box::pin(self.dispatch(instruction, stop)).await?
                    {
                        return Ok(value);
                    }
                }
            }
            if self.pending_callbacks {
                Box::pin(self.drain_pending_callbacks()).await?;
            }
        }
    }

    async fn drain_pending_callbacks(&mut self) -> Result<(), NativeError> {
        let returned_hook = self.returned_hook.take();
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        if self.output_pending() {
            self.drain_output().await?;
        }
        if let Some(returned) = returned_hook {
            self.resume_property_hook(returned).await?;
        }
        self.refresh_pending_callbacks();
        Ok(())
    }

    pub(crate) fn refresh_pending_callbacks(&mut self) {
        self.pending_callbacks = !self.error_handlers.pending.is_empty()
            || self.output_pending()
            || self.returned_hook.is_some();
    }

    /// Backward branches consume the same fairness budget as calls.
    pub(crate) fn backward(&self, target: Target) -> bool {
        self.current()
            .is_some_and(|frame| target.index() < frame.ip())
    }

    /// Reject a target outside the current function as an internal PHP error.
    pub(crate) fn jump(&mut self, target: Target) -> Result<(), NativeError> {
        let length = self
            .current()
            .map_or(0, |frame| frame.function().instructions.len());
        match self.call_stack.current_mut() {
            Some(frame) if target.index() < length => {
                frame.set_ip(target.index());
                Ok(())
            }
            _ => Err(internal_error!("invalid jump")),
        }
    }
}
