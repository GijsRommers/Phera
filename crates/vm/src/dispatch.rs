//! Cold instruction dispatch delegates PHP behavior to concern-specific handlers.

use std::rc::Rc;

use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use phera_value::{PhpArray, Value};

use crate::Vm;

/// Returning can finish a native callback boundary or the main script.
pub(crate) enum DispatchResult {
    Continue,
    Returned(Value),
}

impl DispatchResult {
    pub(crate) fn from_return(value: Option<Value>) -> Self {
        value.map_or(Self::Continue, Self::Returned)
    }
}

impl Vm {
    /// Dispatch one non-scalar instruction after publishing the caller IP.
    pub(crate) async fn dispatch(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<DispatchResult, NativeError> {
        match instruction {
            Instruction::Nop => {}
            Instruction::LoadNull { dst } => self.set(dst, Value::Null),
            Instruction::LoadBool { dst, value } => self.set(dst, Value::Bool(value)),
            Instruction::LoadInt { dst, value } => self.set(dst, Value::Int(value as i64)),
            Instruction::LoadConst { dst, constant } => self.load_constant(dst, constant)?,
            Instruction::Move { dst, src } => self.move_local(dst, src)?,
            Instruction::Clear { reg } | Instruction::UnsetLocal { local: reg } => {
                self.clear_local(reg)?
            }
            Instruction::Assign { local, src } => self.execute_assign(local, src)?,
            Instruction::CheckType { local, ty } => self.check_local_type(local, ty)?,
            Instruction::AssignOp { op, local, src } => self.execute_assign_op(op, local, src)?,
            Instruction::Fetch { dst, path, mode } => self.fetch_path(dst, path, mode)?,
            Instruction::AssignOpPath { op, dst, path, src } => {
                self.assign_op_path(op, dst, path, src)?
            }
            Instruction::IncDecPath { mode, dst, path } => self.inc_dec_path(mode, dst, path)?,
            Instruction::AppendLocal { local, src } => self.append_local(local, src)?,
            Instruction::AssignPath { path, src } => self.assign_path(path, src)?,
            Instruction::MakeReference { dst, path } => self.make_reference(dst, path)?,
            Instruction::BindReference { path, src } => self.bind_reference(path, src)?,
            Instruction::UnsetPath { path } => self.unset_place(&self.path(path)?)?,
            Instruction::BinaryIntBranch {
                op,
                lhs,
                rhs,
                target,
            } => self.binary_int_branch(op, lhs, rhs, target)?,
            Instruction::BinaryInt { op, dst, lhs, rhs } => {
                self.execute_binary_int(op, dst, lhs, rhs)?
            }
            Instruction::Binary { dst, op, lhs, rhs } => self.execute_binary(op, dst, lhs, rhs)?,
            Instruction::Unary { dst, op, src } => self.execute_unary(dst, op, src)?,
            Instruction::Cast { to, dst, src } => self.execute_cast(to, dst, src)?,
            Instruction::IncDecLocal { mode, dst, local } => {
                self.execute_inc_dec(mode, dst, local)?
            }
            Instruction::Echo { src } => self.echo_value(src)?,
            Instruction::Jump { target } => self.execute_jump(target, work)?,
            Instruction::JumpIf { condition, target } => self.jump_if(condition, target, work)?,
            Instruction::JumpIfNot { condition, target } => {
                self.jump_if_not(condition, target, work)?
            }
            Instruction::JumpIfNull { value, target } => self.jump_if_null(value, target)?,
            Instruction::JumpIfNotNull { value, target } => self.jump_if_not_null(value, target)?,
            Instruction::BeginSilence => self.diagnostic_reporter.suppression += 1,
            Instruction::EndSilence => {
                self.diagnostic_reporter.suppression =
                    self.diagnostic_reporter.suppression.saturating_sub(1)
            }
            Instruction::InitCall { function } => self.init_call(function)?,
            Instruction::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            Instruction::NameArgument { name } => self.name_argument(name)?,
            Instruction::SendValue { src } => self.send(Some(src), None)?,
            Instruction::SendLocal { local } => {
                self.send(None, Some(crate::AccessPath::local(local)))?
            }
            Instruction::SendPath { path } => self.send(None, Some(self.path(path)?))?,
            Instruction::SendSpread { src } => self.spread(src)?,
            Instruction::Call { dst } => self.execute_call(dst, work).await?,
            Instruction::MakeCallable { dst } => self.make_callable(dst)?,
            Instruction::ReturnReference { src } => return self.return_reference(src, stop),
            Instruction::Return { src } => return self.return_register(src, stop),
            Instruction::ReturnNull => return self.return_null(stop),
            Instruction::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,
            Instruction::Exit { src } => return self.exit_value(src).map(DispatchResult::Returned),
            Instruction::NewArray { dst, .. } => {
                self.set(dst, Value::Array(Rc::new(PhpArray::new())))
            }
            Instruction::ArrayPush { array, src }
            | Instruction::ArrayPushReference { array, src } => {
                self.array_write(array, None, src, false)?
            }
            Instruction::ArraySet { array, key, src }
            | Instruction::ArraySetReference { array, key, src } => {
                self.array_write(array, Some(key), src, false)?
            }
            Instruction::ArraySpread { array, src } => self.array_write(array, None, src, true)?,
            Instruction::Include { dst, path, kind } => self.include(dst, path, kind).await?,
            Instruction::FetchConstant { dst, constant } => self.fetch_constant(dst, constant)?,
            Instruction::DeclareConstant { name, src } => self.declare_constant(name, src)?,
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
                src,
            } => self.init_static(local, variable, src)?,
            Instruction::IterStart { iter, src, .. } => self.iter_start(iter, src, None)?,
            Instruction::IterStartReference { iter, path, .. } => {
                self.start_reference_iterator(iter, path)?
            }
            Instruction::IterNextKeyed {
                iter,
                key,
                value,
                end,
            } => self.iter_next(iter, Some(key), value, end)?,
            Instruction::IterNext { iter, value, end } => self.iter_next(iter, None, value, end)?,
            Instruction::IterEnd { iter } => self.end_iterator(iter)?,
            Instruction::Raise { detail, .. } => self.raise_match_error(detail)?,
            _ => {
                return Err(NativeError::error(
                    "Phera internal error: unimplemented instruction",
                ));
            }
        }
        Ok(DispatchResult::Continue)
    }
}
