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
    #[inline]
    pub(crate) fn execute<const TRACK_GENERATORS: bool>(
        &mut self,
        instruction: Instruction,
        stop: usize,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        use Instruction as I;
        match instruction {
            I::AssignPathFromLocal { .. }
            | I::MakeReference { .. }
            | I::BindReference { .. }
            | I::SendPath { .. }
            | I::UnsetPath { .. }
            | I::AppendLocal { .. }
                if self.classes.has_dimensions
                    && self.prepare_dimension_instruction(instruction)? =>
            {
                return Ok(Some(BatchExit::Instruction(instruction)));
            }
            I::Call { dst, .. } => return self.call::<TRACK_GENERATORS>(dst, work),
            I::CallDirect {
                function,
                args,
                count,
                dst,
                ..
            } => return self.call_direct_general(function, args, count, dst, work),
            I::GeneratorStart
            | I::Yield { .. }
            | I::YieldFrom { .. }
            | I::GeneratorReturn { .. } => {
                return self.execute_generator(instruction, stop);
            }
            I::Return { src } => return self.return_local(src, instruction, stop),
            I::ReturnNull => return self.return_null(stop),
            I::ReturnMissing => return Err(self.missing_magic_return()),
            I::ReturnChecked { .. } | I::ReturnReference { .. } => {
                return Ok(Some(BatchExit::Instruction(instruction)));
            }
            I::Exit { .. } | I::Include { .. } | I::Eval { .. } => {
                return Ok(Some(BatchExit::Instruction(instruction)));
            }
            I::InitCall { function } => self.init_call(function)?,
            I::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            I::InitMethodCall { receiver, method }
                if TRACK_GENERATORS && self.prepare_ready_native_method(receiver, method) => {}
            I::InitMethodCall { receiver, method } => {
                self.init_method_call::<TRACK_GENERATORS>(receiver, method)?
            }
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
            I::SendValue { src } if TRACK_GENERATORS && self.send_window_argument(src)? => {}
            I::SendLocal { local } if TRACK_GENERATORS && self.send_window_argument(local)? => {}
            I::SendValue { src } => self.send(SentArgument::Value(src))?,
            I::SendLocal { local } => self.send(SentArgument::Local(local))?,
            I::SendPath { path } => {
                if !self.send_path(self.path(path)?)? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::SendSpread { src } => {
                if !self.spread(src)? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::NameArgument { name } => self.name_argument(name)?,
            I::MakeCallable { dst } => self.make_callable(dst)?,
            I::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,

            I::Nop => {}
            I::AttributeDiagnostics => self.function_attribute_diagnostics()?,
            I::LoadNull { dst } => self.set(dst, Value::Null),
            I::LoadBool { dst, value } => self.set(dst, Value::Bool(value)),
            I::LoadInt { dst, value } => self.set(dst, Value::Int(i64::from(value))),
            I::LoadConst { dst, constant } => self.load_constant(dst, constant)?,
            I::Move { dst, src } => self.move_local(dst, src)?,
            I::Clear { reg } | I::UnsetLocal { local: reg } => self.clear_local(reg)?,
            I::Assign { local, src } => return self.execute_assign(local, src),
            I::AssignOp { op, local, src } => return self.execute_assign_op(op, local, src),
            I::CheckType { local, ty } => self.check_local_type(local, ty)?,
            I::IncDecLocal { mode, dst, local } => self.execute_inc_dec(mode, dst, local)?,

            I::BinaryIntBranch {
                op,
                lhs,
                rhs,
                target,
            } => self.binary_int_branch(op, lhs, rhs, target)?,
            I::BinaryInt { op, dst, lhs, rhs } => self.execute_binary_int(op, dst, lhs, rhs)?,
            I::Binary { dst, op, lhs, rhs } => return self.execute_binary(op, dst, lhs, rhs),
            I::Unary { dst, op, src } => self.execute_unary(dst, op, src)?,
            I::BuiltinUnary { op, dst, src } => return self.execute_builtin_unary(op, src, dst),
            I::Cast { to, dst, src } => {
                if to == phera_bytecode::CastKind::String && self.operand(src).is_object() {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.execute_cast(to, dst, src)?;
            }

            I::Fetch { dst, path, mode }
            | I::FetchIndex {
                dst, path, mode, ..
            } => {
                return self.fetch_path(dst, path, mode);
            }
            I::AssignOpPath { op, dst, path, src } => {
                return self.assign_op_path(op, dst, path, src);
            }
            I::IncDecPath { mode, dst, path } => {
                return self.inc_dec_path(mode, dst, path);
            }
            I::AppendLocal { local, src } => self.append_local(local, src)?,
            I::AssignPath { path, src } => {
                return self.assign_path(path, src);
            }
            I::AssignPathFromLocal { path, src, dst } => {
                let handled = if let Some(value) = self.read_defined_local(src) {
                    self.assign_property_site(&self.path(path)?, &value, Some((src, dst)))?
                } else {
                    false
                };
                if !handled {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::MakeReference { dst, path } => {
                if self.path(path)?.has_properties()? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.make_reference(dst, path)?;
            }
            I::BindReference { path, src } => {
                if self.path(path)?.has_properties()? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.bind_reference(path, src)?;
            }
            I::UnsetPath { path } => {
                let place = self.path(path)?;
                if place.has_properties()? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.unset_place(&place)?;
            }

            I::DeclareClass { class } => self.declare_class(class)?,
            I::DynamicInstanceOf { dst, source, class } => {
                self.dynamic_instance_of(dst, source, class)?
            }
            I::FetchClassConstant { dst, class, name } => {
                self.fetch_class_constant(dst, class, name)?
            }
            I::FetchDynamicClassConstant { dst, class, name } => {
                self.fetch_dynamic_class_constant(dst, class, name)?;
            }
            I::FetchClassName { dst, class } => self.fetch_class_name(dst, class)?,
            I::FetchThis { dst } => self.fetch_this(dst)?,
            I::ThisOrNull { dst } => self.this_or_null(dst),
            I::Clone { .. } => return Ok(Some(BatchExit::Instruction(instruction))),
            I::InstanceOf { dst, src, class } => self.instance_of(dst, src, class)?,

            I::CheckReturn { source } => {
                if self.return_string_coercion_needed(&self.raw_local(source)) {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.check_return_register(source)?;
            }
            I::FinallyCall {
                state,
                target,
                reason,
            } => self.call_finally(state, target, reason)?,
            I::FinallyReturn { state } => self.leave_finally(state)?,
            I::FinallyDiscard { state } => self.discard_finally(state),
            I::Throw { source } => return Err(self.throw_operand(source)?),

            I::Echo { src } => {
                if self.operand(src).is_object() {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
                self.echo_value(src)?;
            }
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
                if matches!(&*self.operand(src), Value::Object(object) if object.class.is_traversable())
                {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
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

            I::IterStart { iter, src, .. } => {
                if !self.iter_start(iter, src, None)? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::IterStartReference {
                subject,
                iter,
                path,
                ..
            } => {
                if self.path(path)?.has_properties()?
                    || !self.start_reference_iterator(iter, path, subject)?
                {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::IterNextKeyed {
                iter,
                key,
                value,
                end,
            } => {
                if !self.iter_next::<TRACK_GENERATORS>(iter, Some(key), value, end)? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::IterNext { iter, value, end } => {
                if !self.iter_next::<TRACK_GENERATORS>(iter, None, value, end)? {
                    return Ok(Some(BatchExit::Instruction(instruction)));
                }
            }
            I::IterEnd { iter } => self.end_iterator(iter)?,
            I::IssetLocal { .. } | I::IssetPath { .. } => {
                return Err(internal_error!("unimplemented instruction"));
            }
        }
        Ok(None)
    }

    /// `Call`: enter the prepared PHP function, or let the shell run a native one.
    fn call<const TRACK_GENERATORS: bool>(
        &mut self,
        destination: Reg,
        work: &mut usize,
    ) -> Result<Option<BatchExit>, NativeError> {
        *work += 1;
        use crate::calls::call_execution::CallEntry;
        Ok(match self.enter_prepared_call(Some(destination))? {
            CallEntry::Php => None,
            CallEntry::Native if TRACK_GENERATORS => {
                if let Some(value) = self.finish_ready_native_window()? {
                    self.set(destination, value);
                    None
                } else {
                    return self.call_native(destination);
                }
            }
            CallEntry::Native => return self.call_native(destination),
            CallEntry::ConvertStrings => Some(BatchExit::BindCall(destination)),
        })
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
        if value.is_object() && self.return_string_coercion_needed(&value) {
            return Ok(Some(BatchExit::Instruction(instruction)));
        }
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
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("constant frame"))?
            .unit()
            .clone();
        let name = &unit.names[name.index()];
        if name.eq_ignore_ascii_case("class") {
            let value = self.read_local(class);
            let Value::Object(object) = value else {
                return Err(NativeError::error(format!(
                    "Cannot use \"::class\" on {}",
                    value.type_name()
                )));
            };
            self.set(destination, Value::from(object.class.name()));
            return Ok(());
        }
        let target = self.read_local(class);
        let class = match &target {
            Value::Object(object) => std::borrow::Cow::Borrowed(object.class.name()),
            Value::String(name) => String::from_utf8_lossy(name.as_bytes()),
            _ => {
                return Err(NativeError::error(
                    "Class name must be a valid object or a string",
                ));
            }
        };
        let value = self.class_constant(&class, name)?;
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
