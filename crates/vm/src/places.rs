//! Paths are walked and rebuilt iteratively, applying COW at every array level.
use super::*;
impl Vm {
    fn base_reg(&mut self, p: &Place, create: bool) -> Result<Option<Reg>, NativeError> {
        if !p.dynamic {
            return Ok(Some(p.base));
        }
        let mut d = CollectDiagnostics::default();
        let name = to_php_string(&self.get(p.base), &mut d)?.to_string_lossy();
        self.diagnostics_flush(d);
        let f = self
            .frames
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: place without frame"))?;
        if let Some(i) = f.function().locals.iter().position(|n| n == &name) {
            return Ok(Some(Reg(i as u16)));
        }
        if let Some(r) = f.dynamic.get(&name) {
            return Ok(Some(*r));
        }
        if create {
            let r = Reg(u16::try_from(f.slots.len())
                .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
            f.slots.push(None);
            f.dynamic.insert(name, r);
            Ok(Some(r))
        } else {
            if self.suppression == 0 {
                self.warn(format!("Undefined variable ${name}"))
            }
            Ok(None)
        }
    }
    fn keys(&mut self, p: &Place) -> Result<Vec<Option<ArrayKey>>, NativeError> {
        let mut d = CollectDiagnostics::default();
        let keys = p
            .path
            .iter()
            .map(|r| {
                r.map(|r| ArrayKey::from_value_diag(&self.get(r), &mut d))
                    .transpose()
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.diagnostics_flush(d);
        Ok(keys)
    }
    pub(crate) fn read_place(&mut self, p: &Place, quiet: bool) -> NativeResult {
        let Some(r) = self.base_reg(p, false)? else {
            return Ok(Value::Null);
        };
        let slot = self
            .current()
            .and_then(|f| f.slots.get(r.0 as usize))
            .cloned()
            .flatten();
        let mut v = match slot {
            Some(v) => v.deref().clone(),
            None => {
                if !quiet {
                    let n = self
                        .current()
                        .and_then(|f| f.function().locals.get(r.0 as usize))
                        .cloned()
                        .unwrap_or_default();
                    self.warn(format!("Undefined variable ${n}"));
                }
                Value::Null
            }
        };
        for key in self.keys(p)? {
            let key = key.ok_or_else(|| NativeError::error("Cannot use [] for reading"))?;
            v = match v {
                Value::Array(a) => {
                    if let Some(v) = a.get(&key) {
                        v.deref().clone()
                    } else {
                        if !quiet {
                            self.warn(format!("Undefined array key {}", display_key(&key)))
                        }
                        Value::Null
                    }
                }
                Value::String(s) => {
                    let i = match key {
                        ArrayKey::Int(i) => i,
                        ArrayKey::String(k) => {
                            let n = k.to_string_lossy();
                            n.parse::<i64>().map_err(|_| {
                                NativeError::type_error(
                                    "Cannot access offset of type string on string",
                                )
                            })?
                        }
                    };
                    let i = if i < 0 { s.len() as i64 + i } else { i };
                    if let Some(b) = usize::try_from(i).ok().and_then(|i| s.as_bytes().get(i)) {
                        Value::from(vec![*b])
                    } else {
                        if !quiet {
                            self.warn(format!("Uninitialized string offset {i}"))
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
        Ok(v)
    }
    pub(crate) fn write_place(
        &mut self,
        p: &Place,
        value: Value,
        rebind: bool,
    ) -> Result<(), NativeError> {
        let r = self
            .base_reg(p, true)?
            .ok_or_else(|| NativeError::error("Phera internal error: missing place"))?;
        let keys = self.keys(p)?;
        let root = self
            .frames
            .last_mut()
            .and_then(|f| f.slots.get_mut(r.0 as usize))
            .and_then(Option::take)
            .unwrap_or_default();
        let mut d = CollectDiagnostics::default();
        let updated = update(root, &keys, value, rebind, false, &mut d);
        self.diagnostics_flush(d);
        let updated = updated?;
        self.set(r, updated);
        Ok(())
    }
    pub(crate) fn reference(&mut self, p: &Place) -> NativeResult {
        let v = self.read_place(p, true)?;
        let r = self
            .base_reg(p, true)?
            .ok_or_else(|| NativeError::error("Phera internal error: reference missing"))?;
        let mut raw = self
            .current()
            .and_then(|f| f.slots.get(r.0 as usize))
            .cloned()
            .flatten()
            .unwrap_or_default();
        for key in self.keys(p)? {
            let plain = raw.deref().clone();
            raw = match (plain, key) {
                (Value::Array(a), Some(k)) => a.get(&k).cloned().unwrap_or_default(),
                _ => Value::Null,
            };
        }
        if matches!(raw, Value::Reference(_)) {
            return Ok(raw);
        }
        let reference = Value::Reference(Rc::new(RefCell::new(v)));
        self.write_place(p, reference.clone(), true)?;
        Ok(reference)
    }
    pub(crate) fn unset_place(&mut self, p: &Place) -> Result<(), NativeError> {
        let Some(r) = self.base_reg(p, false)? else {
            return Ok(());
        };
        if p.path.is_empty() {
            if let Some(s) = self
                .frames
                .last_mut()
                .and_then(|f| f.slots.get_mut(r.0 as usize))
            {
                *s = None
            }
            return Ok(());
        }
        let keys = self.keys(p)?;
        let root = self
            .frames
            .last_mut()
            .and_then(|f| f.slots.get_mut(r.0 as usize))
            .and_then(Option::take)
            .unwrap_or_default();
        let mut d = CollectDiagnostics::default();
        let updated = update(root, &keys, Value::Null, true, true, &mut d);
        self.diagnostics_flush(d);
        let updated = updated?;
        self.set(r, updated);
        Ok(())
    }
}
fn display_key(k: &ArrayKey) -> String {
    match k {
        ArrayKey::Int(i) => i.to_string(),
        ArrayKey::String(s) => format!("\"{}\"", s.to_string_lossy()),
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
        if let Value::Reference(r) = current {
            let v = r
                .try_borrow()
                .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))?
                .clone();
            parents.push(Parent::Reference(r));
            current = v;
        }
        if current.is_null() || matches!(current, Value::Bool(false)) {
            current = Value::Array(Rc::new(PhpArray::new()))
        }
        if let Value::String(s) = &current {
            if unset {
                return Err(NativeError::error("Cannot unset string offsets"));
            }
            if rebind && matches!(value, Value::Reference(_)) {
                return Err(NativeError::error(
                    "Cannot create references to/from string offsets",
                ));
            }
            let i = match key {
                Some(ArrayKey::Int(i)) => *i,
                Some(ArrayKey::String(s)) => s.to_string_lossy().parse::<i64>().map_err(|_| {
                    NativeError::type_error("Cannot access offset of type string on string")
                })?,
                None => return Err(NativeError::error("[] operator not supported for strings")),
            };
            let i = if i < 0 { s.len() as i64 + i } else { i };
            let Ok(i) = usize::try_from(i) else {
                diagnostics.warning(format!("Illegal string offset {i}"));
                return Ok(current);
            };
            let mut bytes = s.as_bytes().to_vec();
            if i > 16 * 1024 * 1024 {
                return Err(NativeError::error(
                    "Phera string offset allocation limit exceeded",
                ));
            }
            if bytes.len() <= i {
                bytes.resize(i + 1, b' ')
            }
            parents.push(Parent::String(bytes, i));
            current = Value::Null;
            continue;
        }
        let Value::Array(mut array) = current else {
            return Err(NativeError::error("Cannot use a scalar value as an array"));
        };
        let key = if let Some(k) = key {
            k.clone()
        } else {
            let a = Rc::make_mut(&mut array);
            a.push(Value::Null)?
        };
        current = Rc::make_mut(&mut array)
            .get_mut(&key)
            .map(std::mem::take)
            .unwrap_or_default();
        parents.push(Parent::Array(array, key));
    }
    if let Value::Reference(r) = current {
        if !rebind {
            *r.try_borrow_mut()
                .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))? =
                value;
            current = Value::Reference(r)
        } else {
            current = value
        }
    } else {
        current = value
    }
    let mut remove = unset;
    while let Some(parent) = parents.pop() {
        current = match parent {
            Parent::String(mut bytes, i) => {
                let s = to_php_string(&current, diagnostics)?;
                let Some(b) = s.as_bytes().first() else {
                    return Err(NativeError::error(
                        "Cannot assign an empty string to a string offset",
                    ));
                };
                if s.len() > 1 {
                    diagnostics
                        .warning("Only the first byte will be assigned to the string offset".into())
                }
                if let Some(slot) = bytes.get_mut(i) {
                    *slot = *b;
                }
                Value::from(bytes)
            }
            Parent::Reference(r) => {
                *r.try_borrow_mut().map_err(|_| {
                    NativeError::error("Phera internal error: borrowed reference")
                })? = current;
                Value::Reference(r)
            }
            Parent::Array(mut a, key) => {
                if remove {
                    Rc::make_mut(&mut a).remove(&key);
                    remove = false
                } else {
                    Rc::make_mut(&mut a).insert(key, current);
                }
                Value::Array(a)
            }
        };
    }
    Ok(current)
}
