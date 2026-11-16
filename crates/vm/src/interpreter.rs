//! Copy-only instruction dispatch over heap frames.

use phera_bytecode::{Instruction, Target};
use phera_extension_api::{NativeError, NativeResult};

use crate::Vm;

impl Vm {
    pub(crate) async fn interpret(&mut self, stop: usize) -> NativeResult {
        let mut work = 0usize;
        let mut running_unit = self
            .current()
            .ok_or_else(|| NativeError::error("Phera internal error: missing frame"))?
            .unit
            .clone();
        loop {
            let frame = self
                .current()
                .ok_or_else(|| NativeError::error("Phera internal error: missing frame"))?;
            if !std::sync::Arc::ptr_eq(&running_unit, &frame.unit) {
                running_unit = frame.unit.clone();
            }
            let function = match frame.function_id {
                Some(index) => running_unit.functions.get(index),
                None => Some(&running_unit.main),
            }
            .ok_or_else(|| NativeError::error("Phera internal error: invalid function"))?;
            let instructions = &function.instructions;
            let base = frame.base;
            let mut ip = frame.ip;
            let tracks_arguments = !frame.arguments.is_empty();
            loop {
                let instruction = instructions.get(ip).copied().ok_or_else(|| {
                    NativeError::error("Phera internal error: invalid instruction pointer")
                })?;
                ip += 1;
                self.executed_instructions += 1;
                if crate::fast::scalar_step(
                    instruction,
                    &mut self.registers[base..],
                    &mut ip,
                    tracks_arguments,
                ) {
                    if matches!(
                        instruction,
                        Instruction::Jump { .. }
                            | Instruction::JumpIf { .. }
                            | Instruction::JumpIfNot { .. }
                    ) {
                        work += 1;
                        if work >= 4096 {
                            if let Some(frame) = self.call_stack.frames.last_mut() {
                                frame.ip = ip
                            }
                            work = 0;
                            tokio::task::yield_now().await;
                        }
                    }
                    continue;
                }
                if let Some(frame) = self.call_stack.frames.last_mut() {
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
                let frame = self.current().ok_or_else(|| {
                    NativeError::error("Phera internal error: missing continuation")
                })?;
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

    pub(crate) fn jump(&mut self, target: Target) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.frames.last_mut()
            && target.index() < frame.function().instructions.len()
        {
            frame.ip = target.index();
            return Ok(());
        }
        Err(NativeError::error("Phera internal error: invalid jump"))
    }
}
