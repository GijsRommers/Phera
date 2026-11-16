//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::{Instruction, Target};
use phera_extension_api::{NativeError, NativeResult};

use crate::Vm;

impl Vm {
    /// Retain code and register windows across scalar steps; periodically yield for Tokio fairness.
    pub(crate) async fn interpret(&mut self, stop: usize) -> NativeResult {
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
                        // Bound CPU monopolization on a shared single-thread Tokio worker.
                        if work >= 4096 {
                            if let Some(frame) = self.call_stack.current_mut() {
                                frame.ip = ip
                            }
                            work = 0;
                            tokio::task::yield_now().await;
                        }
                    }
                    continue;
                }
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.ip = ip
                }
                if let crate::dispatch::DispatchResult::Returned(value) =
                    self.dispatch(instruction, stop, &mut work).await?
                {
                    return Ok(value);
                }
                if work >= 4096 {
                    work = 0;
                    tokio::task::yield_now().await;
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
