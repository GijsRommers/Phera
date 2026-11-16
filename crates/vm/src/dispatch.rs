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
            Instruction::DeclareClass { class } => self.declare_class(class)?,
            Instruction::New { class } => self.new_object(class)?,
            Instruction::InitMethodCall { receiver, method } => {
                self.init_method_call(receiver, method)?
            }
            Instruction::FetchClassName { dst, class } => {
                let name = self.class_symbol(class)?;
                self.set(dst, Value::from(name));
            }
            Instruction::FetchThis { dst } => self.fetch_this(dst)?,
            Instruction::Clone { dst, src } => self.clone_object(dst, src)?,
            Instruction::InstanceOf { dst, src, class } => self.instance_of(dst, src, class)?,
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
            Instruction::InitCall { function } => self.init_call(function)?,
            Instruction::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
            Instruction::NameArgument { name } => self.name_argument(name)?,
            Instruction::SendValue { src: source } => self.send(Some(source), None)?,
            Instruction::SendLocal { local } => {
                self.send(None, Some(crate::AccessPath::local(local)))?
            }
            Instruction::SendPath { path } => self.send(None, Some(self.path(path)?))?,
            Instruction::SendSpread { src: source } => self.spread(source)?,
            Instruction::Call { dst: destination } => self.execute_call(destination, work).await?,
            Instruction::MakeCallable { dst: destination } => self.make_callable(destination)?,
            Instruction::ReturnReference { src: source } => {
                return self.return_reference(source, stop);
            }
            Instruction::Return { src: source } => return self.return_register(source, stop),
            Instruction::ReturnNull => return self.return_null(stop),
            Instruction::ReceiveDefault { param, passed } => self.receive_default(param, passed)?,
            Instruction::Exit { src: source } => {
                return self.exit_value(source).map(DispatchResult::Returned);
            }
            Instruction::NewArray {
                dst: destination, ..
            } => self.set(destination, Value::Array(Rc::new(PhpArray::new()))),
            Instruction::ArrayPush { array, src: source }
            | Instruction::ArrayPushReference { array, src: source } => {
                self.array_write(array, None, source, crate::arrays::ArrayWrite::Element)?
            }
            Instruction::ArraySet {
                array,
                key,
                src: source,
            }
            | Instruction::ArraySetReference {
                array,
                key,
                src: source,
            } => self.array_write(array, Some(key), source, crate::arrays::ArrayWrite::Element)?,
            Instruction::ArraySpread { array, src: source } => {
                self.array_write(array, None, source, crate::arrays::ArrayWrite::Spread)?
            }
            Instruction::Include {
                dst: destination,
                path,
                kind,
            } => self.include(destination, path, kind).await?,
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
            Instruction::IterStart {
                iter: iterator,
                src: source,
                ..
            } => self.iter_start(iterator, source, None)?,
            Instruction::IterStartReference {
                subject,
                iter: iterator,
                path,
                ..
            } => self.start_reference_iterator(iterator, path, subject)?,
            Instruction::IterNextKeyed {
                iter: iterator,
                key,
                value,
                end,
            } => self.iter_next(iterator, Some(key), value, end)?,
            Instruction::IterNext {
                iter: iterator,
                value,
                end,
            } => self.iter_next(iterator, None, value, end)?,
            Instruction::IterEnd { iter: iterator } => self.end_iterator(iterator)?,
            Instruction::Raise { detail, .. } => self.raise_match_error(detail)?,
            _ => {
                return Err(internal_error!("unimplemented instruction"));
            }
        }
        Ok(DispatchResult::Continue)
    }
}
