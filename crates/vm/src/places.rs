//! Paths are walked and rebuilt iteratively, applying COW at every array level.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{PathBase, Reg, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Value, to_php_string};

use crate::access_mode::{Binding, Creation, Missing, Mutation};
use crate::{AccessPath, Vm};
use crate::{inline_stack::InlineStack, path_update::update};

impl Vm {
    /// Resolve a dynamic name, optionally creating its register for a write.
    fn base_register(
        &mut self,
        place: &AccessPath,
        creation: Creation,
        missing: Missing,
    ) -> Result<Option<Reg>, NativeError> {
        let base = match place.base()? {
            PathBase::Register(register) => return Ok(Some(register)),
            PathBase::Named(register) => register,
            PathBase::GlobalNamed {
                name: register,
                scratch,
            } => {
                let mut diagnostics = self.diagnostic_reporter.take_operator();
                let result = to_php_string(&self.operand(register), &mut diagnostics);
                self.diagnostics_flush(diagnostics);
                return self.global_register(result?.to_string_lossy(), scratch, creation, missing);
            }
            PathBase::Globals => return Err(internal_error!("global table has no register")),
        };
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let name = to_php_string(&self.operand(base), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("place without frame"))?;
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
        if creation == Creation::Create {
            let register = Reg(u16::try_from(frame.count)
                .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
            self.registers.push_undefined();
            frame.count += 1;
            frame.dynamic.insert(name, register);
            Ok(Some(register))
        } else {
            if missing == Missing::Warn && self.diagnostic_reporter.is_visible() {
                self.warn(format!("Undefined variable ${name}"))
            }
            Ok(None)
        }
    }

    /// Convert path keys in source order while reusing the operator diagnostic buffer.
    fn keys(&mut self, place: &AccessPath) -> Result<InlineStack<Option<ArrayKey>>, NativeError> {
        let mut diagnostics = self.diagnostic_reporter.take_operator();
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
        self.diagnostic_reporter.restore_operator(diagnostics);
        result.map(|()| keys)
    }

    /// Read through PHP references, suppressing missing-slot warnings when requested.
    pub(crate) fn read_place(&mut self, place: &AccessPath, missing: Missing) -> NativeResult {
        if place.base()? == PathBase::Globals {
            return self.global_snapshot();
        }
        let Some(register) = self.base_register(place, Creation::Existing, missing)? else {
            return Ok(Value::Null);
        };
        let mut value = self.read_root(register, missing);
        for key in self.keys(place)?.iter().cloned() {
            let key = key.ok_or_else(|| NativeError::error("Cannot use [] for reading"))?;
            value = match value {
                Value::Array(array) => {
                    if let Some(value) = array.get(&key) {
                        value.deref().clone()
                    } else {
                        if missing == Missing::Warn {
                            self.warn(format!("Undefined array key {}", display_key(&key)))
                        }
                        Value::Null
                    }
                }
                Value::String(string) => self.read_string_offset(&string, key, missing)?,
                other => {
                    if missing == Missing::Warn {
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

    /// Separate shared arrays on writes while preserving reference-cell identity.
    pub(crate) fn write_place(
        &mut self,
        place: &AccessPath,
        value: Value,
        binding: Binding,
    ) -> Result<(), NativeError> {
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("missing place"))?;
        // Appending to an existing array needs neither key conversion nor a
        // parent reconstruction stack. References retain the general handler.
        let mut steps = place.steps()?;
        if matches!(steps.next(), Some(Step::Append))
            && steps.next().is_none()
            && let Some(Value::Array(array)) = self
                .call_stack
                .current_mut()
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
            .current_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let updated = update(root, &keys, value, binding.into(), &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let updated = updated?;
        if let PathBase::GlobalNamed { name, .. } = place.base()? {
            let name = self.get(name);
            if let Value::String(name) = name {
                self.publish_global(name.to_string_lossy(), updated.clone());
            }
        }
        self.set(register, updated);
        Ok(())
    }

    /// Promote a slot to a shared reference cell only when an alias is requested.
    pub(crate) fn reference(&mut self, place: &AccessPath) -> NativeResult {
        let value = self.read_place(place, crate::access_mode::Missing::Silent)?;
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("reference missing"))?;
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
        self.write_place(
            place,
            reference.clone(),
            crate::access_mode::Binding::ByReference,
        )?;
        Ok(reference)
    }

    /// Remove the slot itself without assigning null through an existing alias.
    pub(crate) fn unset_place(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(());
        };
        if place.steps()?.next().is_none() {
            if let PathBase::GlobalNamed { name, .. } = place.base()? {
                let name = self.get(name).deref().clone();
                if let Value::String(name) = name {
                    self.remove_global(&name.to_string_lossy());
                }
            }
            if let Some(string) = self.call_stack.current_mut().and_then(|frame| {
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
            .current_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let updated = update(root, &keys, Value::Null, Mutation::Unset, &mut diagnostics);
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

impl Vm {
    /// Read a root slot without evaluating array keys, retaining undefined-variable warnings.
    fn read_root(&mut self, register: Reg, missing: Missing) -> Value {
        let slot = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.0 as usize))
            .cloned()
            .flatten();
        match slot {
            Some(value) => value.deref().clone(),
            None => {
                if missing == Missing::Warn {
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
        }
    }
}
