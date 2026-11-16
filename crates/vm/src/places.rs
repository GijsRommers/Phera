//! Paths are walked and rebuilt iteratively, applying COW at every array level.
use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{PathBase, Reg, Step};

use crate::{inline_stack::InlineStack, path_update::update};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value, to_php_string};

use crate::{AccessPath, Vm};
impl Vm {
    fn base_reg(&mut self, place: &AccessPath, create: bool) -> Result<Option<Reg>, NativeError> {
        let base = match place.base()? {
            PathBase::Register(register) => return Ok(Some(register)),
            PathBase::Named(register) => register,
        };
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let name = to_php_string(&self.operand(base), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let frame = self
            .call_stack
            .frames
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: place without frame"))?;
        if let Some(index) = frame.function().locals.iter().position(|candidate| {
            frame
                .unit
                .names
                .get(candidate.index())
                .is_some_and(|value| value == &name)
        }) {
            return Ok(Some(Reg(index as u16)));
        }
        if let Some(register) = frame.dynamic.get(&name) {
            return Ok(Some(*register));
        }
        if create {
            let register = Reg(u16::try_from(frame.count)
                .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
            self.registers.push(None);
            frame.count += 1;
            frame.dynamic.insert(name, register);
            Ok(Some(register))
        } else {
            if self.diagnostic_reporter.suppression == 0 {
                self.warn(format!("Undefined variable ${name}"))
            }
            Ok(None)
        }
    }
    fn keys(&mut self, place: &AccessPath) -> Result<InlineStack<Option<ArrayKey>>, NativeError> {
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let mut keys = InlineStack::new();
        let result = (|| {
            for step in place.steps()? {
                let key = match step {
                    Step::Index(register) => Some(ArrayKey::from_value_diag(
                        &self.operand(register),
                        &mut diagnostics,
                    )?),
                    Step::Append => None,
                };
                keys.push(key);
            }
            Ok(())
        })();
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.operator_diagnostics = diagnostics;
        result.map(|()| keys)
    }
    pub(crate) fn read_place(&mut self, place: &AccessPath, quiet: bool) -> NativeResult {
        let Some(register) = self.base_reg(place, false)? else {
            return Ok(Value::Null);
        };
        let slot = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.0 as usize))
            .cloned()
            .flatten();
        let mut value = match slot {
            Some(value) => value.deref().clone(),
            None => {
                if !quiet {
                    let name = self
                        .current()
                        .and_then(|frame| {
                            frame
                                .function()
                                .locals
                                .get(register.0 as usize)
                                .and_then(|name| frame.unit.names.get(name.index()))
                        })
                        .cloned()
                        .unwrap_or_default();
                    self.warn(format!("Undefined variable ${name}"));
                }
                Value::Null
            }
        };
        for key in self.keys(place)?.iter().cloned() {
            let key = key.ok_or_else(|| NativeError::error("Cannot use [] for reading"))?;
            value = match value {
                Value::Array(array) => {
                    if let Some(value) = array.get(&key) {
                        value.deref().clone()
                    } else {
                        if !quiet {
                            self.warn(format!("Undefined array key {}", display_key(&key)))
                        }
                        Value::Null
                    }
                }
                Value::String(string) => {
                    let index = match key {
                        ArrayKey::Int(index) => index,
                        ArrayKey::String(key) => {
                            let name = key.to_string_lossy();
                            name.parse::<i64>().map_err(|_| {
                                NativeError::type_error(
                                    "Cannot access offset of type string on string",
                                )
                            })?
                        }
                    };
                    let index = if index < 0 {
                        string.len() as i64 + index
                    } else {
                        index
                    };
                    if let Some(b) = usize::try_from(index)
                        .ok()
                        .and_then(|index| string.as_bytes().get(index))
                    {
                        Value::from(vec![*b])
                    } else {
                        if !quiet {
                            self.warn(format!("Uninitialized string offset {index}"))
                        }
                        Value::from("")
                    }
                }
                other => {
                    if !quiet {
                        self.warn(format!(
                            "Trying to access array offset on {}",
                            other.type_name()
                        ))
                    }
                    Value::Null
                }
            };
        }
        Ok(value)
    }
    pub(crate) fn write_place(
        &mut self,
        place: &AccessPath,
        value: Value,
        rebind: bool,
    ) -> Result<(), NativeError> {
        let register = self
            .base_reg(place, true)?
            .ok_or_else(|| NativeError::error("Phera internal error: missing place"))?;
        // Appending to an existing array needs neither key conversion nor a
        // parent reconstruction stack. References retain the general handler.
        let mut steps = place.steps()?;
        if matches!(steps.next(), Some(Step::Append))
            && steps.next().is_none()
            && let Some(Value::Array(array)) = self
                .call_stack
                .frames
                .last_mut()
                .and_then(|frame| {
                    frame
                        .slots_mut(&mut self.registers)
                        .get_mut(register.index())
                })
                .and_then(Option::as_mut)
        {
            Rc::make_mut(array).push(value)?;
            return Ok(());
        }
        let keys = self.keys(place)?;
        let root = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let updated = update(root, &keys, value, rebind, false, &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let updated = updated?;
        self.set(register, updated);
        Ok(())
    }
    pub(crate) fn reference(&mut self, place: &AccessPath) -> NativeResult {
        let value = self.read_place(place, true)?;
        let register = self
            .base_reg(place, true)?
            .ok_or_else(|| NativeError::error("Phera internal error: reference missing"))?;
        let mut raw = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.0 as usize))
            .cloned()
            .flatten()
            .unwrap_or_default();
        for key in self.keys(place)?.iter().cloned() {
            let plain = raw.deref().clone();
            raw = match (plain, key) {
                (Value::Array(array), Some(key)) => array.get(&key).cloned().unwrap_or_default(),
                _ => Value::Null,
            };
        }
        if matches!(raw, Value::Reference(_)) {
            return Ok(raw);
        }
        let reference = Value::Reference(Rc::new(RefCell::new(value)));
        self.write_place(place, reference.clone(), true)?;
        Ok(reference)
    }
    pub(crate) fn unset_place(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        let Some(register) = self.base_reg(place, false)? else {
            return Ok(());
        };
        if place.steps()?.next().is_none() {
            if let Some(string) = self.call_stack.frames.last_mut().and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            }) {
                *string = None
            }
            return Ok(());
        }
        let keys = self.keys(place)?;
        let root = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let updated = update(root, &keys, Value::Null, true, true, &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let updated = updated?;
        self.set(register, updated);
        Ok(())
    }
}
fn display_key(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(index) => index.to_string(),
        ArrayKey::String(string) => format!("\"{}\"", string.to_string_lossy()),
    }
}
