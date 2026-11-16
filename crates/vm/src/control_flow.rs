//! PHP control flow instruction handlers, separate from dispatch.

use phera_bytecode::{BinaryOp, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{Value, to_bool};

use crate::Vm;

impl Vm {
    /// Implements PHP Jump semantics outside the dispatch loop.
    pub(crate) fn execute_jump(
        &mut self,
        target: Target,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        if self.backward(target) {
            *work += 1;
        }
        self.jump(target)?;
        Ok(())
    }
    /// Implements PHP JumpIf semantics outside the dispatch loop.
    pub(crate) fn jump_if(
        &mut self,
        condition: Reg,
        target: Target,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        if to_bool(&self.operand(condition)) {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }
    /// Implements PHP JumpIfNot semantics outside the dispatch loop.
    pub(crate) fn jump_if_not(
        &mut self,
        condition: Reg,
        target: Target,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        if !to_bool(&self.operand(condition)) {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }
    /// Implements PHP JumpIfNull semantics outside the dispatch loop.
    pub(crate) fn jump_if_null(&mut self, value: Reg, target: Target) -> Result<(), NativeError> {
        if self.get(value).is_null() {
            self.jump(target)?;
        }
        Ok(())
    }
    /// Implements PHP JumpIfNotNull semantics outside the dispatch loop.
    pub(crate) fn jump_if_not_null(
        &mut self,
        value: Reg,
        target: Target,
    ) -> Result<(), NativeError> {
        if !self.get(value).is_null() {
            self.jump(target)?;
        }
        Ok(())
    }
}

impl Vm {
    /// PHP scalar comparison coercion runs before deciding whether to branch.
    pub(crate) fn binary_int_branch(
        &mut self,
        operator: BinaryOp,
        left: Reg,
        right: i32,
        target: Target,
    ) -> Result<(), NativeError> {
        let left = self.read_local(left);
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let value =
            crate::ops::binary(operator, &left, &Value::Int(right as i64), &mut diagnostics);
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.operator_diagnostics = diagnostics;
        if !to_bool(&value?) {
            self.jump(target)?
        }
        Ok(())
    }
}
