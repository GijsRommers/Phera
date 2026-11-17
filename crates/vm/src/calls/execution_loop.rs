//! The single synchronous frame loop delegates pure scalar work to a small borrowed batch.

use crate::Vm;
use crate::scalar_batches::YIELD_AFTER_STEPS;
use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use phera_value::Value;

/// The async shell handles only suspension, cold operations and completed callbacks.
pub(crate) enum BatchExit {
    Instruction(Instruction),
    NativeCall(phera_bytecode::Reg),
    Returned(Value),
    Yield,
    Handlers,
}

impl Vm {
    /// Retain code once across frame changes; no Rust recursion follows PHP call depth.
    pub(crate) fn run_batch(
        &mut self,
        stop: usize,
        work: &mut usize,
    ) -> Result<BatchExit, NativeError> {
        let mut executed = 0;
        let code = self.code.clone();
        let result = self.run_batch_body(&code, stop, work, &mut executed);
        self.executed_instructions += executed;
        result
    }

    /// Scalar operations and the call protocol share one loop and one borrowed instruction slice.
    fn run_batch_body(
        &mut self,
        code: &crate::code_table::CodeTable,
        stop: usize,
        work: &mut usize,
        executed: &mut u64,
    ) -> Result<BatchExit, NativeError> {
        loop {
            let frame = self
                .current()
                .ok_or_else(|| internal_error!("missing frame"))?;
            let function = code.function(frame.function);
            let base = frame.base;
            let mut ip = frame.ip;
            loop {
                let Some(instruction) = self.next_scalar_instruction(
                    &function.instructions,
                    base,
                    &mut ip,
                    work,
                    executed,
                )?
                else {
                    return Ok(BatchExit::Yield);
                };
                if let Instruction::IterNext { iter, value, end } = instruction
                    && crate::iteration::snapshot_step(
                        iter,
                        value,
                        end,
                        self.call_stack.current().copied(),
                        &mut self.call_stack.extras,
                        &mut self.registers,
                        &mut ip,
                    )
                {
                    continue;
                }
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.ip = ip;
                }
                if let Some(exit) = self.call_step(instruction, stop, work)? {
                    return Ok(exit);
                }
                if !self.error_handlers.pending.is_empty() {
                    return Ok(BatchExit::Handlers);
                }
                if *work >= YIELD_AFTER_STEPS {
                    *work = 0;
                    return Ok(BatchExit::Yield);
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
    /// Publish scalar progress before a diagnostic, frame switch or cooperative yield.
    #[inline]
    fn next_scalar_instruction(
        &mut self,
        instructions: &[Instruction],
        base: usize,
        ip: &mut usize,
        work: &mut usize,
        executed: &mut u64,
    ) -> Result<Option<Instruction>, NativeError> {
        let batch =
            crate::scalar_batches::run(instructions, self.registers.tail_mut(base), *ip, *work);
        *executed += batch.executed;
        *work = batch.work;
        *ip = batch.ip;
        match batch.exit {
            crate::scalar_batches::ScalarExit::Instruction(instruction) => Ok(Some(instruction)),
            crate::scalar_batches::ScalarExit::Yield => {
                if let Some(frame) = self.call_stack.current_mut() {
                    frame.ip = *ip;
                }
                Ok(None)
            }
            crate::scalar_batches::ScalarExit::Invalid => Err(internal_error!("invalid IP")),
        }
    }
}
