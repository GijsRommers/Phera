//! Copy-only instruction dispatch over heap frames.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{IncDec, IncludeKind, Instruction, Target};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{CollectDiagnostics, PhpArray, Value, to_bool, to_php_string};

use crate::{Vm, frame, ops};

impl Vm {
    pub(crate) async fn interpret(&mut self, stop: usize) -> NativeResult {
        let mut work = 0usize;
        loop {
            let instruction = {
                let frame = self
                    .frames
                    .last_mut()
                    .ok_or_else(|| NativeError::error("Phera internal error: missing frame"))?;
                let instruction = frame
                    .function()
                    .instructions
                    .get(frame.ip)
                    .copied()
                    .ok_or_else(|| {
                        NativeError::error("Phera internal error: invalid instruction pointer")
                    })?;
                frame.ip += 1;
                instruction
            };
            self.executed_instructions += 1;
            match instruction {
                Instruction::Nop => {}
                Instruction::LoadNull { dst } => self.set(dst, Value::Null),
                Instruction::LoadBool { dst, value } => self.set(dst, Value::Bool(value)),
                Instruction::LoadInt { dst, value } => self.set(dst, Value::Int(value as i64)),
                Instruction::LoadConst { dst, constant } => {
                    let value = self
                        .current()
                        .and_then(|frame| frame.unit.constants.get(constant.index()))
                        .map(frame::value)
                        .ok_or_else(|| NativeError::error("Phera internal error: constant"))?;
                    self.set(dst, value);
                }
                Instruction::Move { dst, src } => {
                    let value = self.read_local(src);
                    self.set(dst, value);
                }
                Instruction::Clear { reg } | Instruction::UnsetLocal { local: reg } => {
                    if let Some(slot) = self
                        .frames
                        .last_mut()
                        .and_then(|frame| frame.slots_mut(&mut self.registers).get_mut(reg.index()))
                    {
                        *slot = None;
                    }
                }
                Instruction::Assign { local, src } => {
                    let value = self.get(src);
                    if let Some(frame) = self.frames.last_mut()
                        && local.index() < frame.function().parameters.len()
                        && let Some(argument) = frame.arguments.get_mut(local.index())
                    {
                        *argument = value.clone();
                    }
                    self.assign_local(local, value)?;
                }
                Instruction::CheckType { local, ty } => {
                    let ty = self
                        .current()
                        .and_then(|frame| frame.unit.types.get(ty.index()))
                        .cloned()
                        .ok_or_else(|| NativeError::error("Phera internal error: type"))?;
                    let value = crate::types::coerce(self.get(local), &ty, true, self)?;
                    self.assign_local(local, value)?;
                }
                Instruction::AssignOp { op, local, src } => {
                    let reference = self
                        .current()
                        .and_then(|frame| frame.slots(&self.registers).get(local.index()))
                        .and_then(Option::as_ref)
                        .is_some_and(Value::is_reference);
                    if op != phera_bytecode::BinaryOp::Concat && !reference {
                        self.execute_binary(op, local, local, src)?;
                        continue;
                    }
                    if op == phera_bytecode::BinaryOp::Concat && self.assign_concat(local, src)? {
                        continue;
                    }
                    let mut diagnostics = CollectDiagnostics::default();
                    let value = ops::binary(op, &self.get(local), &self.get(src), &mut diagnostics);
                    self.diagnostics_flush(diagnostics);
                    self.assign_local(local, value?)?;
                }
                Instruction::Fetch { dst, path, mode } => {
                    let place = self.path(path)?;
                    let value = self.read_place(&place, mode != phera_bytecode::FetchMode::Read)?;
                    self.set(dst, value);
                }
                Instruction::AssignOpPath { op, dst, path, src } => {
                    let place = self.path(path)?;
                    let left = self.read_place(&place, false)?;
                    let mut diagnostics = CollectDiagnostics::default();
                    let value = ops::binary(op, &left, &self.get(src), &mut diagnostics);
                    self.diagnostics_flush(diagnostics);
                    let value = value?;
                    self.write_place(&place, value.clone(), false)?;
                    self.set(dst, value);
                }
                Instruction::IncDecPath { mode, dst, path } => {
                    let place = self.path(path)?;
                    let old = self.read_place(&place, false)?;
                    let mut diagnostics = CollectDiagnostics::default();
                    let value = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
                        phera_value::increment(&old, &mut diagnostics)
                    } else {
                        phera_value::decrement(&old, &mut diagnostics)
                    };
                    self.diagnostics_flush(diagnostics);
                    let value = value?;
                    self.write_place(&place, value.clone(), false)?;
                    self.set(
                        dst,
                        if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                            old
                        } else {
                            value
                        },
                    );
                }
                Instruction::AssignPath { path, src } => {
                    self.write_place(&self.path(path)?, self.get(src), false)?
                }
                Instruction::MakeReference { dst, path } => {
                    let value = self.reference(&self.path(path)?)?;
                    self.set(dst, value);
                }
                Instruction::BindReference { path, src } => {
                    let value = self
                        .current()
                        .and_then(|frame| frame.slots(&self.registers).get(src.index()))
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or_default();
                    self.write_place(&self.path(path)?, value, true)?;
                }
                Instruction::UnsetPath { path } => self.unset_place(&self.path(path)?)?,
                Instruction::Binary { dst, op, lhs, rhs } => {
                    self.execute_binary(op, dst, lhs, rhs)?
                }
                Instruction::Unary { dst, op, src } => {
                    let mut diagnostics = CollectDiagnostics::default();
                    let value = ops::unary(op, &self.get(src), &mut diagnostics);
                    self.diagnostics_flush(diagnostics);
                    self.set(dst, value?);
                }
                Instruction::Cast { to, dst, src } => self.execute_cast(to, dst, src)?,
                Instruction::IncDecLocal { mode, dst, local } => {
                    self.execute_inc_dec(mode, dst, local)?
                }
                Instruction::Echo { src } => {
                    let mut diagnostics = CollectDiagnostics::default();
                    let text = to_php_string(&self.get(src), &mut diagnostics)?;
                    self.diagnostics_flush(diagnostics);
                    self.out(text.as_bytes());
                }
                Instruction::Jump { target } => {
                    if self.backward(target) {
                        work += 1;
                    }
                    self.jump(target)?;
                }
                Instruction::JumpIf { condition, target } => {
                    if to_bool(&self.get(condition)) {
                        if self.backward(target) {
                            work += 1;
                        }
                        self.jump(target)?;
                    }
                }
                Instruction::JumpIfNot { condition, target } => {
                    if !to_bool(&self.get(condition)) {
                        if self.backward(target) {
                            work += 1;
                        }
                        self.jump(target)?;
                    }
                }
                Instruction::JumpIfNull { value, target } => {
                    if self.get(value).is_null() {
                        self.jump(target)?;
                    }
                }
                Instruction::JumpIfNotNull { value, target } => {
                    if !self.get(value).is_null() {
                        self.jump(target)?;
                    }
                }
                Instruction::BeginSilence => self.suppression += 1,
                Instruction::EndSilence => self.suppression = self.suppression.saturating_sub(1),
                Instruction::InitCall { function } => self.init_call(function)?,
                Instruction::InitDynamicCall { callee } => self.init_dynamic_call(callee)?,
                Instruction::NameArgument { name } => self.name_argument(name)?,
                Instruction::SendValue { src } => self.send(Some(src), None)?,
                Instruction::SendLocal { local } => self.send(
                    None,
                    Some(crate::Place {
                        base: local,
                        dynamic: false,
                        path: vec![],
                    }),
                )?,
                Instruction::SendPath { path } => self.send(None, Some(self.path(path)?))?,
                Instruction::SendSpread { src } => self.spread(src)?,
                Instruction::Call { dst } => {
                    work += 1;
                    self.finish_call(dst).await?;
                }
                Instruction::MakeCallable { dst } => {
                    let call = self
                        .frames
                        .last_mut()
                        .and_then(|frame| frame.calls.pop())
                        .ok_or_else(|| NativeError::error("Phera internal error: callable"))?;
                    self.set(dst, Value::from(call.callee.name.clone()));
                }
                Instruction::ReturnReference { src } => {
                    let value = self
                        .current()
                        .and_then(|frame| frame.slots(&self.registers).get(src.index()))
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or_default();
                    if !value.is_reference() {
                        self.diagnostic(
                            "Notice",
                            "Only variable references should be returned by reference".into(),
                        );
                    }
                    if let Some(value) = self.return_value(value, stop)? {
                        return Ok(value);
                    }
                }
                Instruction::Return { src } => {
                    if let Some(value) = self.return_value(self.get(src), stop)? {
                        return Ok(value);
                    }
                }
                Instruction::ReturnNull => {
                    if let Some(value) = self.return_value(Value::Null, stop)? {
                        return Ok(value);
                    }
                }
                Instruction::ReceiveDefault { param, passed } => {
                    if self
                        .current()
                        .and_then(|frame| frame.slots(&self.registers).get(param.index()))
                        .is_some_and(Option::is_some)
                    {
                        self.jump(passed)?;
                    }
                }
                Instruction::Exit { src } => return self.exit_value(src),
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
                Instruction::ArraySpread { array, src } => {
                    self.array_write(array, None, src, true)?
                }
                Instruction::Include { dst, path, kind } => {
                    self.include(
                        dst,
                        path,
                        matches!(kind, IncludeKind::IncludeOnce | IncludeKind::RequireOnce),
                        matches!(kind, IncludeKind::Require | IncludeKind::RequireOnce),
                    )
                    .await?
                }
                Instruction::FetchConstant { dst, constant } => {
                    let symbol = self
                        .current()
                        .and_then(|frame| frame.unit.symbols.get(constant.index()))
                        .ok_or_else(|| {
                            NativeError::error("Phera internal error: constant symbol")
                        })?;
                    let value = self
                        .constants
                        .get(&symbol.name)
                        .or_else(|| {
                            symbol
                                .fallback
                                .as_ref()
                                .and_then(|name| self.constants.get(name))
                        })
                        .cloned()
                        .ok_or_else(|| {
                            NativeError::error(format!("Undefined constant \"{}\"", symbol.name))
                        })?;
                    self.set(dst, value);
                }
                Instruction::DeclareConstant { name, src } => {
                    let name = self.name(name)?;
                    if self.constants.contains_key(&name) {
                        self.warn(format!("Constant {name} already defined"));
                    } else {
                        self.constants.insert(name, self.get(src));
                    }
                }
                Instruction::DeclareFunction { function } => {
                    let unit = self
                        .current()
                        .ok_or_else(|| NativeError::error("Phera internal error: declaration"))?
                        .unit
                        .clone();
                    let name = unit
                        .functions
                        .get(function.index())
                        .ok_or_else(|| NativeError::error("Phera internal error: function"))?
                        .name
                        .to_ascii_lowercase();
                    if self.functions.contains_key(&name) {
                        return Err(NativeError::error(format!(
                            "Cannot redeclare function {name}()"
                        )));
                    }
                    self.functions.insert(name, (unit, function.index()));
                }
                Instruction::BindDynamicGlobal { name } => {
                    let mut diagnostics = CollectDiagnostics::default();
                    let name = to_php_string(&self.get(name), &mut diagnostics)?.to_string_lossy();
                    self.diagnostics_flush(diagnostics);
                    let local = self.named_slot(&name)?;
                    self.bind_global(local, name)?;
                }
                Instruction::BindGlobal { local, name } => {
                    let name = self.name(name)?;
                    self.bind_global(local, name)?;
                }
                Instruction::BindStatic {
                    local,
                    variable,
                    initialized,
                } => {
                    let key = self.static_key(variable)?;
                    if let Some(value) = self.statics.get(&key).cloned() {
                        self.set(local, value);
                        self.jump(initialized)?;
                    }
                }
                Instruction::InitStatic {
                    local,
                    variable,
                    src,
                } => {
                    let key = self.static_key(variable)?;
                    let value = Value::Reference(Rc::new(RefCell::new(self.get(src))));
                    self.statics.insert(key, value.clone());
                    self.set(local, value);
                }
                Instruction::IterStart { iter, src, .. } => self.iter_start(iter, src, None)?,
                Instruction::IterStartReference { iter, path, .. } => {
                    let place = self.path(path)?;
                    let value = self.read_place(&place, false)?;
                    drop(value);
                    self.iter_start_reference(iter, place)?;
                }
                Instruction::IterNextKeyed {
                    iter,
                    key,
                    value,
                    end,
                } => self.iter_next(iter, Some(key), value, end)?,
                Instruction::IterNext { iter, value, end } => {
                    self.iter_next(iter, None, value, end)?
                }
                Instruction::IterEnd { iter } => {
                    if let Some(frame) = self.frames.last_mut() {
                        frame.iterators.remove(&iter.0);
                    }
                }
                Instruction::Raise { detail, .. } => {
                    let value = self.get(detail);
                    let description = match &value {
                        Value::String(_) => "of type string".to_owned(),
                        Value::Array(_) => "of type array".to_owned(),
                        _ => {
                            let mut diagnostics = CollectDiagnostics::default();
                            to_php_string(&value, &mut diagnostics)?.to_string_lossy()
                        }
                    };
                    return Err(NativeError::throw(
                        "UnhandledMatchError",
                        format!("Unhandled match case {description}"),
                    ));
                }
                _ => {
                    return Err(NativeError::error(
                        "Phera internal error: unimplemented instruction",
                    ));
                }
            }
            if work >= 4096 {
                work = 0;
                tokio::task::yield_now().await;
            }
        }
    }

    fn backward(&self, target: Target) -> bool {
        self.current()
            .is_some_and(|frame| target.index() < frame.ip)
    }

    pub(crate) fn jump(&mut self, target: Target) -> Result<(), NativeError> {
        if let Some(frame) = self.frames.last_mut()
            && target.index() < frame.function().instructions.len()
        {
            frame.ip = target.index();
            return Ok(());
        }
        Err(NativeError::error("Phera internal error: invalid jump"))
    }
}
