//! PHP control flow instruction handlers, separate from dispatch.
//!
//! A branch on a named local reads it with `read_local`, so an undefined
//! variable warns here, on the branch's own line, exactly where the read
//! that used to precede the branch warned.

use phera_bytecode::{BinaryOp, Reg, Target};
use phera_extension_api::NativeError;
use phera_value::{Value, to_bool};

use crate::Vm;
use crate::calls::execution_loop::BatchExit;

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
        if to_bool(&self.read_local(condition)) {
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
        if !to_bool(&self.read_local(condition)) {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }

    /// Implements PHP JumpIfNull semantics outside the dispatch loop.
    pub(crate) fn jump_if_null(&mut self, value: Reg, target: Target) -> Result<(), NativeError> {
        if self.read_local(value).is_null() {
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
        if !self.read_local(value).is_null() {
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
        when: bool,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        let left = self.read_local(left);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let value = crate::ops::binary(
            operator,
            &left,
            &Value::Int(right as i64),
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        if to_bool(&value?) == when {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }

    /// `BinaryBranch`: the comparison is `Binary`'s own, computed into the
    /// frame's reserved scratch slot, and the branch is taken from there, so
    /// there is one definition of the comparison. Anything that cannot finish
    /// synchronously (an unset operand, an object's comparison handler, a
    /// pending string comparison) leaves through a `BatchExit` that completes
    /// the same computation and then decides the branch.
    pub(crate) fn binary_branch(
        &mut self,
        operator: BinaryOp,
        lhs: Reg,
        rhs: Reg,
        target: Target,
        when: bool,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        let scratch = self
            .current()
            .map(|frame| frame.function().registers.saturating_sub(1))
            .unwrap_or(0);
        let scratch = Reg(scratch);
        let exit = match self.execute_binary(operator, scratch, lhs, rhs)? {
            Some(exit) => exit,
            None => return self.take_branch(scratch, target, when, work),
        };
        Ok(Some(match exit {
            BatchExit::UndefinedBinary { op, lhs, rhs, .. } => {
                BatchExit::UndefinedBinaryBranch {
                    op,
                    lhs,
                    rhs,
                    target,
                    when,
                }
            }
            BatchExit::Instruction(phera_bytecode::Instruction::Binary {
                op, lhs, rhs, ..
            }) => BatchExit::Instruction(phera_bytecode::Instruction::BinaryBranch {
                op,
                lhs,
                rhs,
                target,
                when,
            }),
            BatchExit::Compare(pending) => BatchExit::CompareBranch {
                pending,
                target,
                when,
            },
            _ => return Err(internal_error!("unexpected exit for a comparison branch")),
        }))
    }

    /// Take the branch from the comparison result in `result`, then free the
    /// scratch slot again.
    fn take_branch(
        &mut self,
        result: Reg,
        target: Target,
        when: bool,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        let value = self.get(result);
        self.clear_local(result)?;
        if to_bool(&value) == when {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(None)
    }

    /// A compare-and-branch with an unset operand: warn as `Binary` does —
    /// for `>` and `>=` PHP reports the right operand first — then decide.
    pub(crate) async fn binary_branch_with_undefined_operand(
        &mut self,
        op: BinaryOp,
        lhs: Reg,
        rhs: Reg,
        target: Target,
        when: bool,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        let (left_missing, right_missing) =
            if matches!(op, BinaryOp::Greater | BinaryOp::GreaterEqual) {
                let right = self.warn_missing_operand(rhs).await?;
                (self.warn_missing_operand(lhs).await?, right)
            } else {
                let left = self.warn_missing_operand(lhs).await?;
                (left, self.warn_missing_operand(rhs).await?)
            };
        let left = if left_missing {
            Value::Null
        } else {
            self.get(lhs)
        };
        let right = if right_missing {
            Value::Null
        } else {
            self.get(rhs)
        };
        let value = self.binary_with_strings(op, left, right).await?;
        if to_bool(&value) == when {
            if self.backward(target) {
                *work += 1;
            }
            self.jump(target)?;
        }
        Ok(())
    }
}
