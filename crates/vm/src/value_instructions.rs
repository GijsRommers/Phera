//! Synchronous instruction handlers keep PHP semantics inside the shared frame loop.

use crate::{Vm, fast::ScalarStep};
use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use phera_value::Value;

impl Vm {
    /// Fallback local operations retain diagnostics and PHP reference assignment.
    #[inline]
    pub(crate) fn local_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::LoadNull { dst: destination } => self.set(destination, Value::Null),
            Instruction::LoadBool {
                dst: destination,
                value,
            } => self.set(destination, Value::Bool(value)),
            Instruction::LoadInt {
                dst: destination,
                value,
            } => self.set(destination, Value::Int(value as i64)),
            Instruction::LoadConst {
                dst: destination,
                constant,
            } => self.load_constant(destination, constant)?,
            Instruction::Move {
                dst: destination,
                src: source,
            } => self.move_local(destination, source)?,
            Instruction::Clear { reg: register } | Instruction::UnsetLocal { local: register } => {
                self.clear_local(register)?
            }
            Instruction::Assign { local, src: source } => self.execute_assign(local, source)?,
            Instruction::CheckType {
                local,
                ty: declaration_type,
            } => self.check_local_type(local, declaration_type)?,
            Instruction::AssignOp {
                op: operator,
                local,
                src: source,
            } => self.execute_assign_op(operator, local, source)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Writable paths share the same property, alias and missing-value handlers.
    #[inline]
    pub(crate) fn path_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::FetchIndex {
                dst, path, mode, ..
            } => self.fetch_path(dst, path, mode)?,
            Instruction::Fetch {
                dst: destination,
                path,
                mode,
            } => self.fetch_path(destination, path, mode)?,
            Instruction::AssignOpPath {
                op: operator,
                dst: destination,
                path,
                src: source,
            } => self.assign_op_path(operator, destination, path, source)?,
            Instruction::IncDecPath {
                mode,
                dst: destination,
                path,
            } => self.inc_dec_path(mode, destination, path)?,
            Instruction::AppendLocal { local, src: source } => self.append_local(local, source)?,
            Instruction::AssignPath { path, src: source } => self.assign_path(path, source)?,
            Instruction::MakeReference {
                dst: destination,
                path,
            } => self.make_reference(destination, path)?,
            Instruction::BindReference { path, src: source } => {
                self.bind_reference(path, source)?
            }
            Instruction::UnsetPath { path } => self.unset_place(&self.path(path)?)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
    /// Operators that need coercion run after the borrowed scalar batch stops.
    #[inline]
    pub(crate) fn operator_instruction(
        &mut self,
        instruction: Instruction,
    ) -> Result<ScalarStep, NativeError> {
        match instruction {
            Instruction::BinaryIntBranch {
                op: operator,
                lhs: left,
                rhs: right,
                target,
            } => self.binary_int_branch(operator, left, right, target)?,
            Instruction::BinaryInt {
                op: operator,
                dst: destination,
                lhs: left,
                rhs: right,
            } => self.execute_binary_int(operator, destination, left, right)?,
            Instruction::Binary {
                dst: destination,
                op: operator,
                lhs: left,
                rhs: right,
            } => self.execute_binary(operator, destination, left, right)?,
            Instruction::Unary {
                dst: destination,
                op: operator,
                src: source,
            } => self.execute_unary(destination, operator, source)?,
            Instruction::Cast {
                to,
                dst: destination,
                src: source,
            } => self.execute_cast(to, destination, source)?,
            Instruction::IncDecLocal {
                mode,
                dst: destination,
                local,
            } => self.execute_inc_dec(mode, destination, local)?,
            _ => return Ok(ScalarStep::Fallback),
        }
        Ok(ScalarStep::Complete)
    }
}
