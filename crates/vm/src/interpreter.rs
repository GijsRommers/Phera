//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::{Instruction, Target};
use phera_extension_api::{LocalFuture, NativeError, NativeResult};

use crate::Vm;

impl Vm {
    /// One erased engine future per runtime entry keeps procedural batching code
    /// out of the general call loop. PHP-to-PHP calls still use recycled heap
    /// frames; this allocation occurs only at an interpreter boundary.
    pub(crate) fn interpret(&mut self, stop: usize) -> LocalFuture<'_, NativeResult> {
        Box::pin(async move {
            loop {
                let result = if self
                    .current()
                    .is_some_and(|frame| frame.function_id.is_none())
                {
                    if self.error_handlers.error.is_some() {
                        self.interpret_procedural::<true>(stop).await
                    } else {
                        self.interpret_procedural::<false>(stop).await
                    }
                } else {
                    if self.error_handlers.error.is_some() {
                        self.interpret_frames::<true>(stop).await
                    } else {
                        self.interpret_frames::<false>(stop).await
                    }
                };
                match result {
                    Ok(value) => return Ok(value),
                    Err(NativeError::Unwind(payload))
                        if payload.0.is::<crate::error_handlers::HandlerModeChanged>() =>
                    {
                        continue;
                    }
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
    pub(crate) async fn interpret_frames<const USER_ERROR_HANDLERS: bool>(
        &mut self,
        stop: usize,
    ) -> NativeResult {
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
                    Instruction::Return { src } => {
                        if USER_ERROR_HANDLERS {
                            let value = self.read_local(src);
                            if USER_ERROR_HANDLERS && !self.error_handlers.pending.is_empty() {
                                self.drain_return_handlers().await?;
                            }
                            self.return_value(value, stop)
                                .map(crate::dispatch::DispatchResult::from_return)?
                        } else if function.return_type.is_none() {
                            let value = self.read_local(src);
                            self.finish_return_value(value, stop)
                                .map(crate::dispatch::DispatchResult::from_return)?
                        } else {
                            self.return_register(src, stop)?
                        }
                    }
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
                if USER_ERROR_HANDLERS && !self.error_handlers.pending.is_empty() {
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
