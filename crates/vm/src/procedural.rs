//! A small procedural engine isolates scalar-loop code from PHP frame dispatch.

use crate::{Vm, fast::ArgumentTracking, scalar_batches::BatchExit};
use phera_bytecode::Instruction;
use phera_extension_api::NativeResult;
use std::sync::Arc;

impl Vm {
    /// Mirrors the general interpreter at a procedural entry. A user frame or
    /// include switches permanently to the general heap-frame engine; ordinary
    /// PHP function calls therefore never pay a batching branch per instruction.
    pub(crate) async fn interpret_procedural(&mut self, stop: usize) -> NativeResult {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("missing frame"))?
            .unit
            .clone();
        let mut work = 0;
        loop {
            let frame = self
                .current()
                .ok_or_else(|| internal_error!("missing continuation"))?;
            if frame.function_id.is_some() || !Arc::ptr_eq(&unit, &frame.unit) {
                return self.interpret_frames(stop).await;
            }
            let base = frame.base;
            let tracking = if frame.arguments.is_empty() {
                ArgumentTracking::Unobserved
            } else {
                ArgumentTracking::Observed
            };
            let ip = frame.ip;
            let batch = crate::scalar_batches::run(
                &unit.main.instructions,
                self.registers.tail_mut(base),
                ip,
                work,
                tracking,
            );
            self.executed_instructions += batch.executed;
            work = batch.work;
            if let Some(frame) = self.call_stack.current_mut() {
                frame.ip = batch.ip;
            }
            let instruction = match batch.exit {
                BatchExit::Instruction(instruction) => instruction,
                BatchExit::Yield => {
                    tokio::task::yield_now().await;
                    continue;
                }
                BatchExit::Invalid => return Err(internal_error!("invalid instruction pointer")),
            };
            if self.procedural_fast_step(instruction, batch.ip, &mut work) {
                continue;
            }
            if let crate::dispatch::DispatchResult::Returned(value) =
                self.dispatch(instruction, stop, &mut work).await?
            {
                return Ok(value);
            }
            if !self.error_handlers.pending.is_empty() {
                self.drain_error_handlers().await?;
            }
            if work >= 4096 {
                work = 0;
                tokio::task::yield_now().await;
            }
        }
    }

    /// Mirrors the general interpreter's existing snapshot iterator fast path.
    fn procedural_fast_step(
        &mut self,
        instruction: Instruction,
        mut ip: usize,
        work: &mut usize,
    ) -> bool {
        if let Instruction::Call { dst } = instruction
            && self.complete_constructed_call(dst)
        {
            *work += 1;
            return true;
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
            if let Some(frame) = self.call_stack.current_mut() {
                frame.ip = ip;
            }
            return true;
        }
        false
    }
}
