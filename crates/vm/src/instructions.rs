//! One dispatch match selects the handler for an instruction.
//!
//! In `echo f($x + 1)`, the scalar batch handles `+` first; this match selects
//! the call and output handlers directly, without probing unrelated groups.

use crate::Vm;
use crate::calls::execution_loop::BatchExit;
use phera_bytecode::Instruction;
use phera_extension_api::NativeError;
use phera_value::{PhpArray, Value};
use std::rc::Rc;

impl Vm {
    /// Select exactly one handler after the borrowed scalar batch stops.
    pub(crate) fn call_step(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        match instruction {
            Instruction::Call { dst } => {
                *work += 1;
                if !self.enter_prepared_call(Some(dst))? {
                    return Ok(Some(BatchExit::NativeCall(dst)));
                }
            }
            Instruction::Return { src } => {
                let Some(value) = self.read_defined_local(src) else {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                };
                if let Some(value) = self.return_value(value, stop)? {
                    return Ok(Some(BatchExit::Returned(value)));
                }
            }
            Instruction::ReturnNull => {
                if let Some(value) = self.return_value(Value::Null, stop)? {
                    return Ok(Some(BatchExit::Returned(value)));
                }
            }

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

            Instruction::Echo { src } => self.echo_value(src)?,
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

            Instruction::LoadNull { dst } => self.set(dst, Value::Null),
            Instruction::LoadBool { dst, value } => self.set(dst, Value::Bool(value)),
            Instruction::LoadInt { dst, value } => self.set(dst, Value::Int(value as i64)),
            Instruction::LoadConst { dst, constant } => self.load_constant(dst, constant)?,
            Instruction::Move { dst, src } => self.move_local(dst, src)?,
            Instruction::Clear { reg: register } | Instruction::UnsetLocal { local: register } => {
                self.clear_local(register)?
            }
            Instruction::Assign { local, src } => self.execute_assign(local, src)?,
            Instruction::CheckType { local, ty } => self.check_local_type(local, ty)?,
            Instruction::AssignOp { op, local, src } => self.execute_assign_op(op, local, src)?,

            Instruction::FetchIndex {
                dst, path, mode, ..
            } => self.fetch_path(dst, path, mode)?,
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

            Instruction::DeclareClass { class } => self.declare_class(class)?,
            Instruction::DynamicInstanceOf { dst, source, class } => {
                self.dynamic_instance_of(dst, source, class)?
            }
            Instruction::FetchClassConstant { dst, class, name } => {
                let class = self.class_target(class)?;
                let name = self.name(name)?;
                let value = self.class_constant(&class, &name)?;
                self.set(dst, value);
            }
            Instruction::FetchClassName { dst, class } => {
                let name = self.class_symbol_value(class)?;
                self.set(dst, name);
            }
            Instruction::FetchThis { dst } => self.fetch_this(dst)?,
            Instruction::Clone { dst, src } => self.clone_object(dst, src)?,
            Instruction::InstanceOf { dst, src, class } => self.instance_of(dst, src, class)?,
            Instruction::ThisOrNull { dst } => {
                let value = self
                    .current()
                    .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref())
                    .and_then(|context| self.frame_receiver(context).cloned())
                    .map(Value::Object)
                    .unwrap_or_default();
                self.set(dst, value);
            }

            Instruction::Capture { src, kind } => self.capture(src, kind)?,
            Instruction::MakeClosure {
                dst,
                function,
                binding,
            } => {
                self.make_closure(dst, function, binding)?;
            }
            Instruction::NewArray { dst, .. } => {
                self.set(dst, Value::Array(Rc::new(PhpArray::new())))
            }
            Instruction::ArrayPush { array, src }
            | Instruction::ArrayPushReference { array, src } => {
                self.array_write(array, None, src, crate::arrays::ArrayWrite::Element)?
            }
            Instruction::ArraySet { array, key, src }
            | Instruction::ArraySetReference { array, key, src } => {
                self.array_write(array, Some(key), src, crate::arrays::ArrayWrite::Element)?
            }
            Instruction::ArraySpread { array, src } => {
                self.array_write(array, None, src, crate::arrays::ArrayWrite::Spread)?
            }

            Instruction::IterStart { iter, src, .. } => self.iter_start(iter, src, None)?,
            Instruction::IterStartReference {
                subject,
                iter,
                path,
                ..
            } => self.start_reference_iterator(iter, path, subject)?,
            Instruction::IterNextKeyed {
                iter,
                key,
                value,
                end,
            } => self.iter_next(iter, Some(key), value, end)?,
            Instruction::IterNext { iter, value, end } => self.iter_next(iter, None, value, end)?,
            Instruction::IterEnd { iter } => self.end_iterator(iter)?,

            Instruction::InitCall { function } => self.init_call(function)?,
            Instruction::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            Instruction::InitMethodCall { receiver, method } => {
                self.init_method_call(receiver, method)?;
            }
            Instruction::InitStaticCall {
                class,
                method,
                binding,
            } => {
                self.prepare_known_static_call(class, method, binding)?;
            }
            Instruction::DynamicMethodCall { receiver, method } => {
                self.dynamic_method_call(receiver, method)?;
            }
            Instruction::StaticCall {
                class,
                method,
                binding,
            } => {
                self.static_style_call(class, method, binding)?;
            }
            Instruction::New { class } => self.new_object(class)?,
            Instruction::NewDynamic { class } => {
                let name = self.class_target(class)?;
                self.new_named_object(&name)?;
            }

            Instruction::SendValue { src } => {
                self.send(crate::calls::sends::SentArgument::Value(src))?
            }
            Instruction::SendLocal { local } => {
                self.send(crate::calls::sends::SentArgument::Local(local))?;
            }
            Instruction::SendPath { path } => {
                self.send(crate::calls::sends::SentArgument::Path(self.path(path)?))?
            }
            Instruction::SendSpread { src } => self.spread(src)?,
            Instruction::NameArgument { name } => self.name_argument(name)?,
            Instruction::MakeCallable { dst } => self.make_callable(dst)?,
            Instruction::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,
            _ => return Ok(Some(BatchExit::Instruction(instruction))),
        }
        Ok(None)
    }
}
