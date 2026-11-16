//! One async loop executes all PHP frames; PHP calls never recurse here.
use super::*;
impl Vm {
    pub(crate) async fn interpret(&mut self, stop: usize) -> NativeResult {
        let mut ticks = 0usize;
        loop {
            if self.frames.len() <= stop {
                return Ok(Value::Null);
            }
            ticks += 1;
            if ticks.is_multiple_of(2048) {
                tokio::task::yield_now().await;
            }
            let op = {
                let f = self
                    .frames
                    .last_mut()
                    .ok_or_else(|| NativeError::error("Phera internal error: missing frame"))?;
                let op = f
                    .function()
                    .instructions
                    .get(f.ip)
                    .cloned()
                    .ok_or_else(|| {
                        NativeError::error(
                            "Phera internal error: instruction pointer out of bounds",
                        )
                    })?;
                f.ip += 1;
                op
            };
            match op {
                Instruction::Include {
                    dst,
                    path,
                    once,
                    required,
                } => {
                    self.include(dst, path, once, required).await?;
                }
                Instruction::Static {
                    local,
                    name,
                    default,
                } => {
                    let function = self
                        .current()
                        .map(|f| f.function().name.clone())
                        .unwrap_or_default();
                    let v = self
                        .statics
                        .entry((function, name))
                        .or_insert_with(|| {
                            Value::Reference(Rc::new(RefCell::new(frame::value(&default))))
                        })
                        .clone();
                    self.set(local, v);
                }
                Instruction::Increment {
                    dst,
                    src,
                    increment,
                } => {
                    let mut d = CollectDiagnostics::default();
                    let value = self.get(src);
                    let result = if increment {
                        phera_value::increment(&value, &mut d)
                    } else {
                        phera_value::decrement(&value, &mut d)
                    };
                    self.diagnostics_flush(d);
                    self.set(dst, result?);
                }
                Instruction::Load { dst, constant } => {
                    let v = self
                        .current()
                        .and_then(|f| f.function().constants.get(constant))
                        .map(frame::value)
                        .ok_or_else(|| {
                            NativeError::error("Phera internal error: invalid constant")
                        })?;
                    self.set(dst, v)
                }
                Instruction::Move { dst, src } => self.set(dst, self.get(src)),
                Instruction::ReadArgument { dst, place } => {
                    self.diagnostic_capture = Some(vec![]);
                    let result = self.read_place(&place, false);
                    let diagnostics = self.diagnostic_capture.take().unwrap_or_default();
                    if let Some(f) = self.frames.last_mut() {
                        f.argument_diagnostics.insert(dst.0, diagnostics);
                    }
                    self.set(dst, result?);
                }
                Instruction::Reference { dst, place } => {
                    let v = self.reference(&place)?;
                    self.set(dst, v)
                }
                Instruction::Read { dst, place, quiet } => {
                    let v = self.read_place(&place, quiet)?;
                    self.set(dst, v)
                }
                Instruction::Rebind { place, src } => {
                    let raw = self
                        .current()
                        .and_then(|f| f.slots.get(src.0 as usize))
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or_default();
                    self.write_place(&place, raw, true)?;
                }
                Instruction::Write { place, src } => {
                    self.write_place(&place, self.get(src), false)?
                }
                Instruction::Bind { place, source } => {
                    let r = self.reference(&source)?;
                    self.write_place(&place, r, true)?
                }
                Instruction::Unset(p) => self.unset_place(&p)?,
                Instruction::Binary { dst, op, lhs, rhs } => {
                    let mut d = CollectDiagnostics::default();
                    let result = ops::binary(op, &self.get(lhs), &self.get(rhs), &mut d);
                    self.diagnostics_flush(d);
                    self.set(dst, result?);
                }
                Instruction::Unary { dst, op, src } => {
                    let mut d = CollectDiagnostics::default();
                    let result = ops::unary(op, &self.get(src), &mut d);
                    self.diagnostics_flush(d);
                    self.set(dst, result?);
                }
                Instruction::Echo(r) => {
                    let mut d = CollectDiagnostics::default();
                    let s = to_php_string(&self.get(r), &mut d);
                    self.diagnostics_flush(d);
                    self.out(s?.as_bytes())
                }
                Instruction::Jump(t) => self.jump(t)?,
                Instruction::Branch {
                    value,
                    target,
                    when,
                } => {
                    if to_bool(&self.get(value)) == when {
                        self.jump(target)?
                    }
                }
                Instruction::BranchNull { value, target } => {
                    if self.get(value).is_null() {
                        self.jump(target)?
                    }
                }
                Instruction::Suppress(on) => {
                    if on {
                        self.suppression += 1
                    } else {
                        self.suppression = self.suppression.saturating_sub(1)
                    }
                }
                Instruction::Return(r) => {
                    let mut v = self.get(r);
                    if let Some((ty, name, strict)) = self.current().and_then(|f| {
                        f.function().return_type.as_ref().map(|t| {
                            (
                                t.clone(),
                                f.function().name.clone(),
                                f.function().strict_types,
                            )
                        })
                    }) {
                        v = types::coerce(v, &ty, strict, self).map_err(|e| match e {
                            NativeError::Throw { class, message } => NativeError::throw(
                                class,
                                format!("{name}(): Return value {message}"),
                            ),
                            e => e,
                        })?;
                    }
                    let frame = self.frames.pop().ok_or_else(|| {
                        NativeError::error("Phera internal error: return without frame")
                    })?;
                    if self.frames.len() == stop {
                        return Ok(v);
                    }
                    if let Some(dst) = frame.destination {
                        self.set(dst, v)
                    }
                }
                Instruction::Exit(r) => {
                    let v = self.get(r);
                    if matches!(
                        v,
                        Value::Int(_) | Value::Bool(_) | Value::Null | Value::Float(_)
                    ) {
                        let mut d = CollectDiagnostics::default();
                        self.status = phera_value::to_int(&v, &mut d)? as i32;
                        self.diagnostics_flush(d);
                    } else if !v.is_string() {
                        return Err(NativeError::type_error(format!(
                            "exit(): Argument #1 ($status) must be of type string|int, {} given",
                            v.type_name()
                        )));
                    } else {
                        let mut d = CollectDiagnostics::default();
                        let s = to_php_string(&v, &mut d)?;
                        self.diagnostics_flush(d);
                        self.out(s.as_bytes())
                    }
                    self.frames.clear();
                    return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                        (),
                    ))));
                }
                Instruction::Callable {
                    dst,
                    name,
                    fallback,
                } => {
                    let mut d = CollectDiagnostics::default();
                    let mut name = to_php_string(&self.get(name), &mut d)?.to_string_lossy();
                    self.diagnostics_flush(d);
                    if !self.exists(&name)
                        && let Some(f) = fallback
                    {
                        name = f;
                    }
                    if !self.exists(&name) {
                        return Err(NativeError::error(format!(
                            "Call to undefined function {name}()"
                        )));
                    }
                    self.set(dst, Value::from(name));
                }
                Instruction::Call {
                    dst,
                    name,
                    fallback,
                    arguments,
                } => {
                    let v = self.get(name);
                    let mut d = CollectDiagnostics::default();
                    let mut name = to_php_string(&v, &mut d)?.to_string_lossy();
                    self.diagnostics_flush(d);
                    if !self.exists(&name)
                        && let Some(f) = fallback
                    {
                        name = f;
                    }
                    let args = self.arguments(&name, arguments)?;
                    if let Some((unit, id)) =
                        self.functions.get(&name.to_ascii_lowercase()).cloned()
                    {
                        self.push_call(unit, id, Some(dst), args)?;
                    } else {
                        let v = self.native(&name, args).await?;
                        self.set(dst, v)
                    }
                }
                Instruction::Array(r) => self.set(r, Value::Array(Rc::new(PhpArray::new()))),
                Instruction::ArrayPush {
                    array,
                    key,
                    value,
                    spread,
                } => {
                    let mut v = self.get(array);
                    let Value::Array(a) = &mut v else {
                        return Err(NativeError::error("Phera internal error: array expected"));
                    };
                    let a = Rc::make_mut(a);
                    let val = self
                        .current()
                        .and_then(|f| f.slots.get(value.0 as usize))
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or_default();
                    if spread {
                        let Value::Array(src) = val else {
                            return Err(NativeError::error(
                                "Only arrays and Traversables can be unpacked",
                            ));
                        };
                        for (k, v) in src.iter() {
                            match k {
                                ArrayKey::Int(_) => {
                                    a.push(v.clone())?;
                                }
                                _ => {
                                    a.insert(k.clone(), v.clone());
                                }
                            }
                        }
                    } else if let Some(k) = key {
                        let mut d = CollectDiagnostics::default();
                        let k = ArrayKey::from_value_diag(&self.get(k), &mut d)?;
                        self.diagnostics_flush(d);
                        a.insert(k, val);
                    } else {
                        a.push(val)?;
                    }
                    self.set(array, v)
                }
                Instruction::Constant {
                    dst,
                    name,
                    fallback,
                } => {
                    let v = self
                        .constants
                        .get(&name)
                        .or_else(|| fallback.as_ref().and_then(|n| self.constants.get(n)))
                        .cloned()
                        .ok_or_else(|| {
                            NativeError::error(format!("Undefined constant \"{name}\""))
                        })?;
                    self.set(dst, v)
                }
                Instruction::Define { name, src } => {
                    if self.constants.contains_key(&name) {
                        self.warn(format!("Constant {name} already defined"))
                    } else {
                        self.constants.insert(name, self.get(src));
                    }
                }
                Instruction::Declare(id) => {
                    let unit = self.current().map(|f| f.unit.clone()).ok_or_else(|| {
                        NativeError::error("Phera internal error: declaration without frame")
                    })?;
                    let f = unit.functions.get(id).ok_or_else(|| {
                        NativeError::error("Phera internal error: function missing")
                    })?;
                    let name = f.name.to_ascii_lowercase();
                    if self.functions.contains_key(&name) {
                        return Err(NativeError::error(format!(
                            "Cannot redeclare function {}()",
                            f.name
                        )));
                    }
                    self.functions.insert(name, (unit, id));
                }
                Instruction::Global { local, name } => {
                    let root = self.frames.first_mut().and_then(|f| {
                        let i = f.function().locals.iter().position(|n| n == &name)?;
                        let slot = f.slots.get_mut(i)?;
                        let v = slot.take().unwrap_or_default();
                        let r = if matches!(v, Value::Reference(_)) {
                            v
                        } else {
                            Value::Reference(Rc::new(RefCell::new(v)))
                        };
                        *slot = Some(r.clone());
                        Some(r)
                    });
                    if let Some(r) = root {
                        self.globals.insert(name.clone(), r);
                    }
                    let value = self.globals.entry(name).or_default();
                    let r = match value {
                        Value::Reference(_) => value.clone(),
                        _ => {
                            let r = Value::Reference(Rc::new(RefCell::new(value.clone())));
                            *value = r.clone();
                            r
                        }
                    };
                    self.set(local, r)
                }
                Instruction::IterInit { id, src, place } => {
                    let v = self.get(src);
                    let snapshot = if let Value::Array(a) = v {
                        Some(a)
                    } else {
                        self.warn(format!(
                            "foreach() argument must be of type array|object, {} given",
                            v.type_name()
                        ));
                        None
                    };
                    if let Some(f) = self.frames.last_mut() {
                        f.iterators.insert(
                            id.0,
                            frame::IteratorState {
                                snapshot: if place.is_none() { snapshot } else { None },
                                source: place,
                                position: None,
                                started: false,
                            },
                        );
                    }
                }
                Instruction::IterNext {
                    id,
                    key,
                    value,
                    end,
                } => {
                    let mut state = self
                        .frames
                        .last_mut()
                        .and_then(|f| f.iterators.remove(&id.0))
                        .ok_or_else(|| {
                            NativeError::error("Phera internal error: missing iterator")
                        })?;
                    let array = if let Some(p) = &state.source {
                        match self.read_place(p, true)? {
                            Value::Array(a) => Some(a),
                            _ => None,
                        }
                    } else {
                        state.snapshot.clone()
                    };
                    let position = array.as_ref().and_then(|a| {
                        if state.started {
                            state.position.and_then(|p| a.next_position(p))
                        } else {
                            a.first_position()
                        }
                    });
                    state.started = true;
                    state.position = position;
                    let entry = array
                        .as_ref()
                        .and_then(|a| position.and_then(|p| a.entry_at(p)))
                        .map(|(k, v)| (k.clone(), v.deref().clone()));
                    drop(array);
                    if let Some((k, mut v)) = entry {
                        self.set(
                            key,
                            match k {
                                ArrayKey::Int(i) => Value::Int(i),
                                ArrayKey::String(s) => Value::String(s),
                            },
                        );
                        if let Some(p) = &state.source {
                            let mut p = p.clone();
                            p.path.push(Some(key));
                            v = self.reference(&p)?;
                        }
                        self.set(value, v);
                    } else {
                        self.jump(end)?
                    }
                    if let Some(f) = self.frames.last_mut() {
                        f.iterators.insert(id.0, state);
                    }
                }
                Instruction::Fatal { class, message } => {
                    return Err(NativeError::throw(
                        Box::leak(class.into_boxed_str()),
                        message,
                    ));
                }
            }
        }
    }
    fn jump(&mut self, t: Target) -> Result<(), NativeError> {
        if let Some(f) = self.frames.last_mut()
            && t.0 < f.function().instructions.len()
        {
            f.ip = t.0;
            return Ok(());
        }
        Err(NativeError::error("Phera internal error: invalid jump"))
    }
}
