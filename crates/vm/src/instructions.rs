//! Synchronous instruction handlers keep PHP semantics inside the shared frame loop.

use crate::{Vm, fast::ScalarStep};
use phera_bytecode::Instruction;
use phera_extension_api::NativeError;

impl Vm {
    /// Every PHP activation uses these same handlers; only suspension leaves this loop.
    pub(crate) fn dispatch_instruction(
        &mut self,
        instruction: Instruction,
        work: &mut usize,
    ) -> Result<ScalarStep, NativeError> {
        if self.object_instruction(instruction)? == ScalarStep::Complete
            || self.local_instruction(instruction)? == ScalarStep::Complete
            || self.path_instruction(instruction)? == ScalarStep::Complete
            || self.operator_instruction(instruction)? == ScalarStep::Complete
            || self.exception_instruction(instruction)? == ScalarStep::Complete
            || self.branch_instruction(instruction, work)? == ScalarStep::Complete
            || self.array_instruction(instruction)? == ScalarStep::Complete
            || self.scope_instruction(instruction)? == ScalarStep::Complete
        {
            Ok(ScalarStep::Complete)
        } else {
            self.iteration_instruction(instruction)
        }
    }

    /// Exception instructions publish their continuation before any observable work.
    #[inline]
    pub(crate) fn exception_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::CheckReturn { source } => {
                let value = self
                    .current()
                    .and_then(|frame| frame.slots(&self.registers).get(source.index()))
                    .and_then(Option::as_ref)
                    .cloned()
                    .unwrap_or_default();
                let value = self.check_return_value(value)?;
                self.set(source, value);
            }
            Instruction::FinallyCall {
                state,
                target,
                reason,
            } => self.enter_finally(
                state,
                target,
                crate::finally::Continuation::Resume {
                    target: self.current().map_or(0, |frame| frame.ip),
                    reason,
                },
            )?,
            Instruction::FinallyReturn { state } => self.leave_finally(state)?,
            Instruction::FinallyDiscard { state } => self.discard_finally(state),
            Instruction::Throw { source } => return Err(self.throw_operand(source)?),
            Instruction::Nop => {}
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Slow branches preserve operand diagnostics and the cooperative work budget.
    #[inline]
    pub(crate) fn branch_instruction(
        &mut self,
        instruction: Instruction,
        work: &mut usize,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::Echo { src: source } => self.echo_value(source)?,
            Instruction::Jump { target } => self.execute_jump(target, work)?,
            Instruction::JumpIf { condition, target } => self.jump_if(condition, target, work)?,
            Instruction::JumpIfNot { condition, target } => {
                self.jump_if_not(condition, target, work)?
            }
            Instruction::JumpIfNull { value, target } => self.jump_if_null(value, target)?,
            Instruction::JumpIfNotNull { value, target } => self.jump_if_not_null(value, target)?,
            Instruction::BeginSilence => self.diagnostic_reporter.begin_silence(),
            Instruction::EndSilence => self.diagnostic_reporter.end_silence(),
            Instruction::Raise { detail, .. } => self.raise_match_error(detail)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Declarations and binding operations update the active request scope.
    #[inline]
    pub(crate) fn scope_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::FetchConstant {
                dst: destination,
                constant,
            } => self.fetch_constant(destination, constant)?,
            Instruction::DeclareConstant { name, src: source } => {
                self.declare_constant(name, source)?
            }
            Instruction::DeclareFunction { function } => self.declare_function(function)?,
            Instruction::BindDynamicGlobal { name } => self.bind_dynamic_global(name)?,
            Instruction::BindGlobal { local, name } => self.bind_named_global(local, name)?,
            Instruction::BindStatic {
                local,
                variable,
                initialized,
            } => self.bind_static(local, variable, initialized)?,
            Instruction::InitStatic {
                local,
                variable,
                src: source,
            } => self.init_static(local, variable, source)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
}
