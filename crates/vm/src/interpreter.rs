//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::{Instruction, Target};
use phera_extension_api::{LocalFuture, NativeError, NativeResult};

use crate::Vm;

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
                let at_top_level = self
                    .current()
                    .is_some_and(|frame| frame.function_id.is_none());
                let result = if at_top_level {
                    self.interpret_procedural(stop).await
                } else {
                    self.interpret_frames(stop).await
                };
                match result {
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
    /// Retain code and register windows across scalar steps; periodically yield for Tokio fairness.
    pub(crate) async fn interpret_frames(&mut self, stop: usize) -> NativeResult {
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
            // Keep the instruction pointer local until a slow instruction or suspension.
            let mut ip = frame.ip;
            let tracking = if frame.arguments.is_empty() {
                crate::fast::ArgumentTracking::Unobserved
            } else {
                crate::fast::ArgumentTracking::Observed
            };
            loop {
                let instruction = instructions
                    .get(ip)
                    .copied()
                    .ok_or_else(|| internal_error!("invalid instruction pointer"))?;
                ip += 1;
                self.executed_instructions += 1;
                if crate::fast::scalar_step(
                    instruction,
                    self.registers.tail_mut(base),
                    &mut ip,
                    tracking,
                ) == crate::fast::ScalarStep::Complete
                {
                    if matches!(
                        instruction,
                        Instruction::Jump { .. }
                            | Instruction::JumpIf { .. }
                            | Instruction::JumpIfNot { .. }
                    ) {
                        work += 1;
                        if work >= 4096 {
                            if let Some(frame) = self.call_stack.current_mut() {
                                frame.ip = ip;
                            }
                            work = 0;
                            tokio::task::yield_now().await;
                        }
                    }
                    continue;
                }
                if let Instruction::FetchIndex {
                    dst,
                    base: array,
                    index,
                    ..
                } = instruction
                    && crate::fast::array_index_step(
                        dst,
                        array,
                        index,
                        self.registers.tail_mut(base),
                    ) == crate::fast::ScalarStep::Complete
                {
                    continue;
                }
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
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.ip = ip
                }
                // Mirrors the corresponding dispatcher arms, preserving the published
                // IP and continuation handling while keeping synchronous hot operations
                // out of the cold async dispatcher future.
                let result = match instruction {
                    Instruction::InitCall { function } => {
                        self.init_call(function)?;
                        crate::dispatch::DispatchResult::Continue
                    }
                    Instruction::SendValue { src } => {
                        self.send_value(src)?;
                        crate::dispatch::DispatchResult::Continue
                    }
                    Instruction::SendLocal { local } => {
                        self.send(None, Some(crate::AccessPath::local(local)))?;
                        crate::dispatch::DispatchResult::Continue
                    }
                    Instruction::Call { dst } => {
                        self.execute_call(dst, &mut work).await?;
                        crate::dispatch::DispatchResult::Continue
                    }
                    Instruction::Return { src } => match self.read_defined_local(src) {
                        // No declared return type: nothing to check or convert.
                        Some(value) if function.return_type.is_none() => self
                            .finish_return_value(value, stop)
                            .map(crate::dispatch::DispatchResult::from_return)?,
                        Some(value) => self
                            .return_value(value, stop)
                            .map(crate::dispatch::DispatchResult::from_return)?,
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
                        crate::dispatch::DispatchResult::Continue
                    }
                    _ => self.dispatch(instruction, stop, &mut work).await?,
                };
                if let crate::dispatch::DispatchResult::Returned(value) = result {
                    return Ok(value);
                }
                if work >= 4096 {
                    work = 0;
                    tokio::task::yield_now().await;
                }
                if !self.error_handlers.pending.is_empty() {
                    self.drain_error_handlers().await?;
                }
                let frame = self
                    .current()
                    .ok_or_else(|| internal_error!("missing continuation"))?;
                if frame.base != base {
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
