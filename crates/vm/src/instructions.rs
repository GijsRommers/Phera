//! Every instruction the interpreter loop does not finish itself, in one `match`.
//!
//! The loop in `calls::execution_loop` runs simple instructions, direct calls
//! and plain returns on the registers it holds. Anything else comes here,
//! with the frame's position already published, and each arm calls the
//! handler that implements it. The few instructions that must wait for
//! something (an include, a native function, a return while an error
//! handler is pending) go back to the async shell in `interpreter.rs`.

use phera_bytecode::{Instruction, Reg};
use phera_extension_api::NativeError;
use phera_value::{PhpArray, Value};

use crate::Vm;
use crate::arrays::ArrayWrite;
use crate::calls::execution_loop::BatchExit;
use crate::calls::sends::SentArgument;

impl Vm {
    /// Runs one instruction. `Some` hands control back to the async shell:
    /// a native call to run, a finished callback, or an instruction to await.
    pub(crate) fn execute(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        use Instruction as I;
        match instruction {
            I::Call { dst } => return self.call(dst, work),
            I::CallDirect {
                function,
                args,
                count,
                dst,
            } => return self.call_direct_general(function, args, count, dst, work),
            I::Return { src } => return self.return_local(src, instruction, stop),
            I::ReturnNull => return self.return_null(stop),
            I::ReturnChecked { .. } | I::ReturnReference { .. } => {
                return Ok(Some(BatchExit::Instruction(instruction)));
            }
            I::Exit { .. } | I::Include { .. } => {
                return Ok(Some(BatchExit::Instruction(instruction)));
            }
            I::InitCall { function } => self.init_call(function)?,
            I::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            I::InitMethodCall { receiver, method } => self.init_method_call(receiver, method)?,
            I::InitStaticCall {
                class,
                method,
                binding,
            } => self.prepare_known_static_call(class, method, binding)?,
            I::DynamicMethodCall { receiver, method } => {
                self.dynamic_method_call(receiver, method)?
            }
            I::StaticCall {
                class,
                method,
                binding,
            } => self.static_style_call(class, method, binding)?,
            I::New { class } => self.new_object(class)?,
            I::NewDynamic { class } => self.new_dynamic_object(class)?,
            I::SendValue { src } => self.send(SentArgument::Value(src))?,
            I::SendLocal { local } => self.send(SentArgument::Local(local))?,
            I::SendPath { path } => self.send(SentArgument::Path(self.path(path)?))?,
            I::SendSpread { src } => self.spread(src)?,
            I::NameArgument { name } => self.name_argument(name)?,
            I::MakeCallable { dst } => self.make_callable(dst)?,
            I::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,

            I::Nop => {}
            I::LoadNull { dst } => self.set(dst, Value::Null),
            I::LoadBool { dst, value } => self.set(dst, Value::Bool(value)),
            I::LoadInt { dst, value } => self.set(dst, Value::Int(i64::from(value))),
            I::LoadConst { dst, constant } => self.load_constant(dst, constant)?,
            I::Move { dst, src } => self.move_local(dst, src)?,
            I::Clear { reg } | I::UnsetLocal { local: reg } => self.clear_local(reg)?,
            I::Assign { local, src } => self.execute_assign(local, src)?,
            I::AssignOp { op, local, src } => self.execute_assign_op(op, local, src)?,
            I::CheckType { local, ty } => self.check_local_type(local, ty)?,
            I::IncDecLocal { mode, dst, local } => self.execute_inc_dec(mode, dst, local)?,

            I::BinaryIntBranch {
                op,
                lhs,
                rhs,
                target,
            } => self.binary_int_branch(op, lhs, rhs, target)?,
            I::BinaryInt { op, dst, lhs, rhs } => self.execute_binary_int(op, dst, lhs, rhs)?,
            I::Binary { dst, op, lhs, rhs } => self.execute_binary(op, dst, lhs, rhs)?,
            I::Unary { dst, op, src } => self.execute_unary(dst, op, src)?,
            I::Cast { to, dst, src } => self.execute_cast(to, dst, src)?,

            I::Fetch { dst, path, mode }
            | I::FetchIndex {
                dst, path, mode, ..
            } => self.fetch_path(dst, path, mode)?,
            I::AssignOpPath { op, dst, path, src } => self.assign_op_path(op, dst, path, src)?,
            I::IncDecPath { mode, dst, path } => self.inc_dec_path(mode, dst, path)?,
            I::AppendLocal { local, src } => self.append_local(local, src)?,
            I::AssignPath { path, src } => self.assign_path(path, src)?,
            I::MakeReference { dst, path } => self.make_reference(dst, path)?,
            I::BindReference { path, src } => self.bind_reference(path, src)?,
            I::UnsetPath { path } => self.unset_place(&self.path(path)?)?,

            I::DeclareClass { class } => self.declare_class(class)?,
            I::DynamicInstanceOf { dst, source, class } => {
                self.dynamic_instance_of(dst, source, class)?
            }
            I::FetchClassConstant { dst, class, name } => {
                self.fetch_class_constant(dst, class, name)?
            }
            I::FetchClassName { dst, class } => self.fetch_class_name(dst, class)?,
            I::FetchThis { dst } => self.fetch_this(dst)?,
            I::ThisOrNull { dst } => self.this_or_null(dst),
            I::Clone { dst, src } => self.clone_object(dst, src)?,
            I::InstanceOf { dst, src, class } => self.instance_of(dst, src, class)?,

            I::CheckReturn { source } => self.check_return_register(source)?,
            I::FinallyCall {
                state,
                target,
                reason,
            } => self.call_finally(state, target, reason)?,
            I::FinallyReturn { state } => self.leave_finally(state)?,
            I::FinallyDiscard { state } => self.discard_finally(state),
            I::Throw { source } => return Err(self.throw_operand(source)?),

            I::Echo { src } => self.echo_value(src)?,
            I::Jump { target } => self.execute_jump(target, work)?,
            I::JumpIf { condition, target } => self.jump_if(condition, target, work)?,
            I::JumpIfNot { condition, target } => self.jump_if_not(condition, target, work)?,
            I::JumpIfNull { value, target } => self.jump_if_null(value, target)?,
            I::JumpIfNotNull { value, target } => self.jump_if_not_null(value, target)?,
            I::BeginSilence => self.diagnostic_reporter.begin_silence(),
            I::EndSilence => self.diagnostic_reporter.end_silence(),
            I::Raise { detail, .. } => self.raise_match_error(detail)?,

            I::Capture { src, kind } => self.capture(src, kind)?,
            I::MakeClosure {
                dst,
                function,
                binding,
            } => self.make_closure(dst, function, binding)?,
            I::NewArray { dst, .. } => self.set(dst, Value::Array(PhpArray::new().into())),
            I::ArrayPush { array, src } | I::ArrayPushReference { array, src } => {
                self.array_write(array, None, src, ArrayWrite::Element)?
            }
            I::ArraySet { array, key, src } | I::ArraySetReference { array, key, src } => {
                self.array_write(array, Some(key), src, ArrayWrite::Element)?
            }
            I::ArraySpread { array, src } => {
                self.array_write(array, None, src, ArrayWrite::Spread)?
            }

            I::FetchConstant { dst, constant } => self.fetch_constant(dst, constant)?,
            I::DeclareConstant { name, src } => self.declare_constant(name, src)?,
            I::DeclareFunction { function } => self.declare_function(function)?,
            I::BindDynamicGlobal { name } => self.bind_dynamic_global(name)?,
            I::BindGlobal { local, name } => self.bind_named_global(local, name)?,
            I::BindStatic {
                local,
                variable,
                initialized,
            } => self.bind_static(local, variable, initialized)?,
            I::InitStatic {
                local,
                variable,
                src,
            } => self.init_static(local, variable, src)?,

            I::IterStart { iter, src, .. } => self.iter_start(iter, src, None)?,
            I::IterStartReference {
                subject,
                iter,
                path,
                ..
            } => self.start_reference_iterator(iter, path, subject)?,
            I::IterNextKeyed {
                iter,
                key,
                value,
                end,
            } => self.iter_next(iter, Some(key), value, end)?,
            I::IterNext { iter, value, end } => self.iter_next(iter, None, value, end)?,
            I::IterEnd { iter } => self.end_iterator(iter)?,
            I::IssetLocal { .. } | I::IssetPath { .. } => {
                return Err(internal_error!("unimplemented instruction"));
            }
        }
        Ok(None)
    }

    /// `Call`: enter the prepared PHP function, or let the shell run a native one.
    fn call(
        &mut self,
        destination: Reg,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        *work += 1;
        if self.enter_prepared_call(Some(destination))? {
            return Ok(None);
        }
        Ok(Some(BatchExit::NativeCall(destination)))
    }

    /// `Return` of a register. An undefined variable warns first, which may
    /// run an error handler, so the shell does that one.
    fn return_local(
        &mut self,
        source: Reg,
        instruction: Instruction,
        stop: usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        let Some(value) = self.read_defined_local(source) else {
            return Ok(Some(BatchExit::Instruction(instruction)));
        };
        Ok(self.return_value(value, stop)?.map(BatchExit::Returned))
    }

    /// `return;` and the end of a function without `return`.
    fn return_null(&mut self, stop: usize) -> Result<Option<BatchExit>, NativeError> {
        Ok(self
            .return_value(Value::Null, stop)?
            .map(BatchExit::Returned))
    }

    /// `$this`, or null outside an object context.
    fn this_or_null(&mut self, destination: Reg) {
        let value = self
            .current()
            .and_then(|frame| frame.this().cloned())
            .map(Value::Object)
            .unwrap_or_default();
        self.set(destination, value);
    }

    /// `Foo::BAR` and `$class::BAR`.
    fn fetch_class_constant(
        &mut self,
        destination: Reg,
        class: Reg,
        name: phera_bytecode::NameId,
    ) -> Result<(), NativeError> {
        let class = self.class_target(class)?;
        let name = self.name(name)?;
        let value = self.class_constant(&class, &name)?;
        self.set(destination, value);
        Ok(())
    }

    /// `Foo::class`, `self::class` and friends.
    fn fetch_class_name(
        &mut self,
        destination: Reg,
        class: phera_bytecode::SymbolId,
    ) -> Result<(), NativeError> {
        let name = self.class_symbol_value(class)?;
        self.set(destination, name);
        Ok(())
    }

    /// `new $class`.
    fn new_dynamic_object(&mut self, class: Reg) -> Result<(), NativeError> {
        let name = self.class_target(class)?;
        self.new_named_object(&name)
    }

    /// Check a pending return value before `finally` runs.
    fn check_return_register(&mut self, source: Reg) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        let value = self.check_return_value(value)?;
        self.set(source, value);
        Ok(())
    }

    /// Enter a `finally` block, to continue after it at the next instruction.
    fn call_finally(
        &mut self,
        state: Reg,
        target: phera_bytecode::Target,
        reason: phera_bytecode::FinallyEntry,
    ) -> Result<(), NativeError> {
        let resume = self.current().map_or(0, |frame| frame.ip());
        let continuation = crate::finally::Continuation::Resume {
            target: resume,
            reason,
        };
        self.enter_finally(state, target, continuation)
    }
}
