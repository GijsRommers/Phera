//! Paths are walked and rebuilt iteratively, applying COW at every array level.
use std::{cell::RefCell, rc::Rc};

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, CollectDiagnostics, Diagnostics, PhpArray, Value, to_php_string};

use crate::{Place, Vm};
impl Vm {
    fn base_reg(&mut self, place: &Place, create: bool) -> Result<Option<Reg>, NativeError> {
        if !place.dynamic {
            return Ok(Some(place.base));
        }
        let mut diagnostics = CollectDiagnostics::default();
        let name = to_php_string(&self.get(place.base), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let frame = self
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
            if self.suppression == 0 {
                self.warn(format!("Undefined variable ${name}"))
            }
            Ok(None)
        }
    }
    fn keys(&mut self, place: &Place) -> Result<Vec<Option<ArrayKey>>, NativeError> {
        let mut diagnostics = CollectDiagnostics::default();
        let keys = place
            .path
            .iter()
            .map(|register| {
                register
                    .map(|register| {
                        ArrayKey::from_value_diag(&self.get(register), &mut diagnostics)
                    })
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.diagnostics_flush(diagnostics);
        Ok(keys)
    }
    pub(crate) fn read_place(&mut self, place: &Place, quiet: bool) -> NativeResult {
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
        for key in self.keys(place)? {
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
        place: &Place,
        value: Value,
        rebind: bool,
    ) -> Result<(), NativeError> {
        let register = self
            .base_reg(place, true)?
            .ok_or_else(|| NativeError::error("Phera internal error: missing place"))?;
        let keys = self.keys(place)?;
        let root = self
            .frames
            .last_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = CollectDiagnostics::default();
        let updated = update(root, &keys, value, rebind, false, &mut diagnostics);
        self.diagnostics_flush(diagnostics);
        let updated = updated?;
        self.set(register, updated);
        Ok(())
    }
    pub(crate) fn reference(&mut self, place: &Place) -> NativeResult {
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
        for key in self.keys(place)? {
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
    pub(crate) fn unset_place(&mut self, place: &Place) -> Result<(), NativeError> {
        let Some(register) = self.base_reg(place, false)? else {
            return Ok(());
        };
        if place.path.is_empty() {
            if let Some(string) = self.frames.last_mut().and_then(|frame| {
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
            .frames
            .last_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = CollectDiagnostics::default();
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
enum Parent {
    String(Vec<u8>, usize),
    Array(Rc<PhpArray>, ArrayKey),
    Reference(Rc<RefCell<Value>>),
}
fn update(
    mut current: Value,
    keys: &[Option<ArrayKey>],
    value: Value,
    rebind: bool,
    unset: bool,
    diagnostics: &mut dyn Diagnostics,
) -> NativeResult {
    let mut parents = vec![];
    for key in keys {
        if let Value::Reference(register) = current {
            let value = std::mem::take(
                &mut *register
                    .try_borrow_mut()
                    .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))?,
            );
            parents.push(Parent::Reference(register));
            current = value;
        }
        if current.is_null() || matches!(current, Value::Bool(false)) {
            current = Value::Array(Rc::new(PhpArray::new()))
        }
        if let Value::String(string) = &current {
            if unset {
                return Err(NativeError::error("Cannot unset string offsets"));
            }
            if rebind && matches!(value, Value::Reference(_)) {
                return Err(NativeError::error(
                    "Cannot create references to/from string offsets",
                ));
            }
            let index = match key {
                Some(ArrayKey::Int(index)) => *index,
                Some(ArrayKey::String(string)) => {
                    string.to_string_lossy().parse::<i64>().map_err(|_| {
                        NativeError::type_error("Cannot access offset of type string on string")
                    })?
                }
                None => return Err(NativeError::error("[] operator not supported for strings")),
            };
            let index = if index < 0 {
                string.len() as i64 + index
            } else {
                index
            };
            let Ok(index) = usize::try_from(index) else {
                diagnostics.warning(format!("Illegal string offset {index}"));
                return Ok(current);
            };
            let mut bytes = string.as_bytes().to_vec();
            if index > 16 * 1024 * 1024 {
                return Err(NativeError::error(
                    "Phera string offset allocation limit exceeded",
                ));
            }
            if bytes.len() <= index {
                bytes.resize(index + 1, b' ')
            }
            parents.push(Parent::String(bytes, index));
            current = Value::Null;
            continue;
        }
        let Value::Array(mut array) = current else {
            return Err(NativeError::error("Cannot use a scalar value as an array"));
        };
        let key = if let Some(key) = key {
            key.clone()
        } else {
            let array = Rc::make_mut(&mut array);
            array.push(Value::Null)?
        };
        current = Rc::make_mut(&mut array)
            .get_mut(&key)
            .map(std::mem::take)
            .unwrap_or_default();
        parents.push(Parent::Array(array, key));
    }
    if let Value::Reference(register) = current {
        if !rebind {
            *register
                .try_borrow_mut()
                .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))? =
                value;
            current = Value::Reference(register)
        } else {
            current = value
        }
    } else {
        current = value
    }
    let mut remove = unset;
    while let Some(parent) = parents.pop() {
        current = match parent {
            Parent::String(mut bytes, index) => {
                let string = to_php_string(&current, diagnostics)?;
                let Some(b) = string.as_bytes().first() else {
                    return Err(NativeError::error(
                        "Cannot assign an empty string to a string offset",
                    ));
                };
                if string.len() > 1 {
                    diagnostics
                        .warning("Only the first byte will be assigned to the string offset".into())
                }
                if let Some(slot) = bytes.get_mut(index) {
                    *slot = *b;
                }
                Value::from(bytes)
            }
            Parent::Reference(register) => {
                *register.try_borrow_mut().map_err(|_| {
                    NativeError::error("Phera internal error: borrowed reference")
                })? = current;
                Value::Reference(register)
            }
            Parent::Array(mut array, key) => {
                if remove {
                    Rc::make_mut(&mut array).remove(&key);
                    remove = false
                } else {
                    Rc::make_mut(&mut array).insert(key, current);
                }
                Value::Array(array)
            }
        };
    }
    Ok(current)
}
