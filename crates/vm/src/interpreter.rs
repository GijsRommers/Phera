//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::{Instruction, Target};
use phera_extension_api::{LocalFuture, NativeError, NativeResult};

use crate::Vm;
use crate::dispatch::DispatchResult;
use crate::fast::ArgumentTracking;
use crate::scalar_batches::{BatchExit, YIELD_AFTER_STEPS};

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
        Box::pin(async move {
            loop {
                match self.interpret_frames(stop).await {
                    Ok(value) => return Ok(value),
                    Err(error) => {
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
    /// Runs the current frame, and every frame it calls, until the call stack
    /// is back at depth `stop` or an error leaves.
    ///
    /// Each round has two parts:
    ///
    /// 1. `scalar_batches::run` executes simple instructions (arithmetic on
    ///    integers and floats, comparisons, jumps, copies) in a tight loop.
    ///    It is a plain function, not async, so the compiler keeps its
    ///    variables in CPU registers. It stops at the first instruction it
    ///    cannot do.
    /// 2. That instruction is executed here: the common ones for calls and
    ///    returns directly, everything else through `dispatch`.
    ///
    /// The outer loop runs again whenever the current frame changes (a call
    /// pushed one, or a return popped one).
    pub(crate) async fn interpret_frames(&mut self, stop: usize) -> NativeResult {
        // Counts loop iterations and calls, so a long-running script gives
        // other requests on this thread a turn every few thousand steps.
        let mut work = 0usize;
        let mut running_unit = self
            .current()
            .ok_or_else(|| internal_error!("missing frame"))?
            .unit
            .clone();
        loop {
            let frame = self
                .current()
                .ok_or_else(|| internal_error!("missing frame"))?;
            if !std::sync::Arc::ptr_eq(&running_unit, &frame.unit) {
                running_unit = frame.unit.clone();
            }
            let function = match frame.function_id {
                Some(index) => running_unit.functions.get(index),
                None => Some(&running_unit.main),
            }
            .ok_or_else(|| internal_error!("invalid function"))?;
            let instructions = &function.instructions;
            let base = frame.base;
            let mut ip = frame.ip;
            let tracking = if frame.arguments.is_empty() {
                ArgumentTracking::Unobserved
            } else {
                ArgumentTracking::Observed
            };
            loop {
                let batch = crate::scalar_batches::run(
                    instructions,
                    self.registers.tail_mut(base),
                    ip,
                    work,
                    tracking,
                );
                self.executed_instructions += batch.executed;
                work = batch.work;
                ip = batch.ip;
                let instruction = match batch.exit {
                    BatchExit::Instruction(instruction) => instruction,
                    BatchExit::Yield => {
                        if let Some(frame) = self.call_stack.current_mut() {
                            frame.ip = ip;
                        }
                        tokio::task::yield_now().await;
                        continue;
                    }
                    BatchExit::Invalid => {
                        return Err(internal_error!("invalid instruction pointer"));
                    }
                };
                // `foreach` over a plain array copy needs no frame access.
                if let Instruction::IterNext { iter, value, end } = instruction
                    && crate::iteration::snapshot_step(
                        iter,
                        value,
                        end,
                        self.call_stack.current_mut(),
                        &mut self.registers,
                        &mut ip,
                    )
                {
                    continue;
                }
                // From here on other code may read or change the frame, so it
                // has to know where execution is.
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.ip = ip
                }
                // The instructions of a call and a return are handled here
                // without going through the general dispatcher, because they
                // are by far the most frequent slow instructions.
                let result = match instruction {
                    Instruction::InitCall { function } => {
                        self.init_call(function)?;
                        DispatchResult::Continue
                    }
                    Instruction::SendValue { src } => {
                        self.send_value(src)?;
                        DispatchResult::Continue
                    }
                    Instruction::SendLocal { local } => {
                        self.send(None, Some(crate::AccessPath::local(local)))?;
                        DispatchResult::Continue
                    }
                    Instruction::Call { dst } => {
                        self.execute_call(dst, &mut work).await?;
                        DispatchResult::Continue
                    }
                    Instruction::Return { src } => match self.read_defined_local(src) {
                        // No declared return type: nothing to check or convert.
                        Some(value) if function.return_type.is_none() => self
                            .finish_return_value(value, stop)
                            .map(DispatchResult::from_return)?,
                        Some(value) => self
                            .return_value(value, stop)
                            .map(DispatchResult::from_return)?,
                        // `return $undefined;` warns. The general dispatcher runs a
                        // user error handler while this frame is still on the stack,
                        // which is what PHP does and what the stack trace shows.
                        None => self.dispatch(instruction, stop, &mut work).await?,
                    },
                    Instruction::ReturnNull => self.return_null(stop)?,
                    Instruction::IterNext {
                        iter: iterator,
                        value,
                        end,
                    } => {
                        self.iter_next(iterator, None, value, end)?;
                        DispatchResult::Continue
                    }
                    _ => self.dispatch(instruction, stop, &mut work).await?,
                };
                if let DispatchResult::Returned(value) = result {
                    return Ok(value);
                }
                if work >= YIELD_AFTER_STEPS {
                    work = 0;
                    tokio::task::yield_now().await;
                }
                // A warning raised by that instruction may be waiting for the
                // user's error handler (`set_error_handler`).
                if !self.error_handlers.pending.is_empty() {
                    self.drain_error_handlers().await?;
                }
                let frame = self
                    .current()
                    .ok_or_else(|| internal_error!("missing continuation"))?;
                if frame.base != base {
                    // A call or a return changed the current frame.
                    break;
                }
                ip = frame.ip;
            }
        }
    }

    pub(crate) fn backward(&self, target: Target) -> bool {
        self.current()
            .is_some_and(|frame| target.index() < frame.ip)
    }

    /// Reject a target outside the current function as an internal PHP error.
    pub(crate) fn jump(&mut self, target: Target) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.current_mut()
            && target.index() < frame.function().instructions.len()
        {
            frame.ip = target.index();
            return Ok(());
        }
        Err(internal_error!("invalid jump"))
    }
}
