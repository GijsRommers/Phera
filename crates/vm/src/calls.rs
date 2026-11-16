//! Call preparation, signature handling and VM intrinsics.
use super::*;
impl Vm {
    pub(crate) fn named_slot(&mut self, name: &str) -> Result<Reg, NativeError> {
        let f = self
            .frames
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: no variable frame"))?;
        if let Some(i) = f.function().locals.iter().position(|n| n == name) {
            return Ok(Reg(i as u16));
        }
        if let Some(r) = f.dynamic.get(name) {
            return Ok(*r);
        }
        let r = Reg(u16::try_from(f.slots.len())
            .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
        f.slots.push(None);
        f.dynamic.insert(name.into(), r);
        Ok(r)
    }
    pub(crate) fn exists(&self, name: &str) -> bool {
        self.functions.contains_key(&name.to_ascii_lowercase())
            || self.registry.get_function(name).is_some()
            || matches!(
                name.to_ascii_lowercase().as_str(),
                "define"
                    | "extract"
                    | "compact"
                    | "get_defined_vars"
                    | "call_user_func"
                    | "call_user_func_array"
                    | "is_callable"
                    | "defined"
                    | "constant"
                    | "function_exists"
                    | "error_reporting"
                    | "ini_set"
                    | "ini_get"
                    | "func_num_args"
                    | "func_get_args"
                    | "func_get_arg"
            )
    }
    pub(crate) fn arguments(
        &mut self,
        name: &str,
        args: Vec<Argument>,
    ) -> Result<Vec<Value>, NativeError> {
        let params: Vec<(String, bool, bool)> =
            if let Some((u, id)) = self.functions.get(&name.to_ascii_lowercase()) {
                u.functions
                    .get(*id)
                    .map(|f| {
                        f.parameters
                            .iter()
                            .map(|p| (p.name.clone(), p.by_reference, p.variadic))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                self.registry
                    .get_function(name)
                    .map(|e| {
                        e.params
                            .iter()
                            .map(|p| (p.name.into(), p.by_ref, p.variadic))
                            .collect()
                    })
                    .unwrap_or_default()
            };
        let mut out: Vec<Option<Value>> = vec![];
        let mut named = false;
        for arg in args {
            if arg.spread {
                let v = self.get(arg.value);
                let Value::Array(a) = v else {
                    return Err(NativeError::type_error(
                        "Only arrays and Traversables can be unpacked",
                    ));
                };
                for (_, v) in a.iter() {
                    out.push(Some(v.deref().clone()))
                }
                continue;
            }
            let index = if let Some(n) = arg.name {
                named = true;
                params
                    .iter()
                    .position(|p| p.0 == n)
                    .ok_or_else(|| NativeError::error(format!("Unknown named parameter ${n}")))?
            } else {
                if named {
                    return Err(NativeError::error(
                        "Cannot use positional argument after named argument",
                    ));
                }
                out.len()
            };
            if out.get(index).is_some_and(Option::is_some) {
                return Err(NativeError::error(format!(
                    "Named parameter ${} overwrites previous argument",
                    params.get(index).map(|p| p.0.as_str()).unwrap_or("?")
                )));
            }
            let by_ref = params
                .get(index)
                .or_else(|| params.last().filter(|p| p.2))
                .is_some_and(|p| p.1);
            let warnings = self
                .frames
                .last_mut()
                .and_then(|f| f.argument_diagnostics.remove(&arg.value.0))
                .unwrap_or_default();
            if !by_ref {
                for bytes in warnings {
                    self.out(&bytes)
                }
            }
            let value = if by_ref {
                if let Some(p) = arg.place {
                    self.reference(&p)?
                } else {
                    return Err(NativeError::error(format!(
                        "{name}(): Argument #{} cannot be passed by reference",
                        index + 1
                    )));
                }
            } else {
                self.get(arg.value)
            };
            if out.len() <= index {
                out.resize(index + 1, None)
            }
            if let Some(slot) = out.get_mut(index) {
                *slot = Some(value)
            }
        }
        out.into_iter()
            .enumerate()
            .map(|(i, v)| {
                if let Some(v) = v {
                    return Ok(v);
                }
                if let Some((u, id)) = self.functions.get(&name.to_ascii_lowercase())
                    && let Some(default) = u
                        .functions
                        .get(*id)
                        .and_then(|f| f.parameters.get(i))
                        .and_then(|p| p.default.as_ref())
                {
                    return Ok(frame::value(default));
                }
                if self
                    .registry
                    .get_function(name)
                    .and_then(|e| e.params.get(i))
                    .is_some_and(|p| p.nullable)
                {
                    return Ok(Value::Null);
                }
                Err(NativeError::error("Phera cannot skip that parameter yet"))
            })
            .collect()
    }
    pub(crate) fn push_call(
        &mut self,
        unit: Arc<CompiledUnit>,
        id: usize,
        dst: Option<Reg>,
        args: Vec<Value>,
    ) -> Result<(), NativeError> {
        if self.frames.len() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit
            )));
        }
        let mut f = Frame::new(unit, Some(id), dst);
        f.arguments = args.clone();
        let params = f.function().parameters.clone();
        for (i, p) in params.iter().enumerate() {
            let value = if p.variadic {
                let mut a = PhpArray::new();
                for v in args.iter().skip(i) {
                    a.push(v.clone())?;
                }
                Value::Array(Rc::new(a))
            } else {
                args.get(i).cloned().or_else(||p.default.as_ref().map(frame::value)).ok_or_else(||NativeError::argument_count_error(format!("Too few arguments to function {}(), {} passed and at least {} expected",f.function().name,args.len(),params.iter().filter(|p|p.default.is_none()&&!p.variadic).count())))?
            };
            let value = if let Some(ty) = &p.ty {
                if !p.by_reference && !p.variadic {
                    let (file, line) = self.location();
                    types::coerce(value,ty,self.strict_types(),self).map_err(|e|match e{NativeError::Throw{class,message}=>NativeError::throw(class,format!("{}(): Argument #{} (${}) {message}, called in {file} on line {line}",f.function().name,i+1,p.name)),e=>e})?
                } else {
                    value
                }
            } else {
                value
            };
            f.set(Reg(i as u16), value);
        }
        self.frames.push(f);
        Ok(())
    }
    pub(crate) async fn native(&mut self, name: &str, mut args: Vec<Value>) -> NativeResult {
        let arg = args.first().cloned().unwrap_or_default();
        let mut d = CollectDiagnostics::default();
        match name.to_ascii_lowercase().as_str() {
            "is_callable" => {
                let name = to_php_string(&arg, &mut d)?.to_string_lossy();
                return Ok(Value::Bool(self.exists(&name)));
            }
            "call_user_func" => {
                let rest = args.iter().skip(1).cloned().collect();
                return self.call(&arg, rest).await;
            }
            "call_user_func_array" => {
                let a = args.get(1).cloned().unwrap_or_default();
                let Value::Array(a) = a else {
                    return Err(NativeError::type_error(
                        "call_user_func_array(): Argument #2 ($args) must be of type array",
                    ));
                };
                return self.call(&arg, a.values().cloned().collect()).await;
            }
            "get_defined_vars" => {
                let mut a = PhpArray::new();
                if let Some(f) = self.current() {
                    for (i, name) in f.function().locals.iter().enumerate() {
                        if let Some(Some(v)) = f.slots.get(i) {
                            a.insert(ArrayKey::String(name.clone().into()), v.deref().clone());
                        }
                    }
                    for (name, r) in &f.dynamic {
                        if let Some(Some(v)) = f.slots.get(r.0 as usize) {
                            a.insert(ArrayKey::String(name.clone().into()), v.deref().clone());
                        }
                    }
                }
                return Ok(Value::Array(Rc::new(a)));
            }
            "compact" => {
                let mut names: Vec<_> = args.into_iter().rev().collect();
                let mut a = PhpArray::new();
                while let Some(v) = names.pop() {
                    match v {
                        Value::Array(values) => names.extend(
                            values
                                .values()
                                .cloned()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev(),
                        ),
                        Value::String(name) => {
                            let n = name.to_string_lossy();
                            let r = self.current().and_then(|f| {
                                f.function()
                                    .locals
                                    .iter()
                                    .position(|x| x == &n)
                                    .map(|i| Reg(i as u16))
                                    .or_else(|| f.dynamic.get(&n).copied())
                            });
                            if let Some(r) = r {
                                a.insert(ArrayKey::String(name), self.get(r));
                            } else {
                                self.warn(format!("compact(): Undefined variable ${n}"));
                            }
                        }
                        _ => {}
                    }
                }
                return Ok(Value::Array(Rc::new(a)));
            }
            "extract" => {
                let Value::Array(a) = arg else {
                    return Err(NativeError::type_error(
                        "extract(): Argument #1 ($array) must be of type array",
                    ));
                };
                let mut count = 0;
                for (k, v) in a.iter() {
                    if let ArrayKey::String(n) = k {
                        let name = n.to_string_lossy();
                        if !name.bytes().enumerate().all(|(i, b)| {
                            b == b'_'
                                || b.is_ascii_alphabetic()
                                || b >= 128
                                || (i > 0 && b.is_ascii_digit())
                        }) || name.is_empty()
                        {
                            continue;
                        }
                        let r = self.named_slot(&name)?;
                        self.set(r, v.deref().clone());
                        count += 1;
                    }
                }
                return Ok(Value::Int(count));
            }
            "define" => {
                let n = to_php_string(&arg, &mut d)?.to_string_lossy();
                if self.constants.contains_key(&n) {
                    self.warn(format!("Constant {n} already defined"));
                    return Ok(Value::Bool(false));
                }
                self.constants
                    .insert(n, args.get(1).cloned().unwrap_or_default());
                return Ok(Value::Bool(true));
            }
            "defined" => {
                return Ok(Value::Bool(
                    self.constants
                        .contains_key(&to_php_string(&arg, &mut d)?.to_string_lossy()),
                ));
            }
            "constant" => {
                let n = to_php_string(&arg, &mut d)?.to_string_lossy();
                return self
                    .constants
                    .get(&n)
                    .cloned()
                    .ok_or_else(|| NativeError::error(format!("Undefined constant \"{n}\"")));
            }
            "function_exists" => {
                return Ok(Value::Bool(
                    self.exists(&to_php_string(&arg, &mut d)?.to_string_lossy()),
                ));
            }
            "func_num_args" => {
                return Ok(Value::Int(
                    self.current().map_or(0, |f| f.arguments.len()) as i64
                ));
            }
            "func_get_args" => {
                let a = self
                    .current()
                    .map(|f| f.arguments.clone())
                    .unwrap_or_default();
                let mut result = PhpArray::new();
                for v in a {
                    result.push(v)?;
                }
                return Ok(Value::Array(Rc::new(result)));
            }
            "func_get_arg" => {
                let i = phera_value::to_int(&arg, &mut d)?;
                return self.current().and_then(|f|f.arguments.get(i as usize)).cloned().ok_or_else(||NativeError::value_error("func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function"));
            }
            "error_reporting" => return Ok(Value::Int(30719)),
            "ini_set" | "ini_get" => return Ok(Value::from("1")),
            _ => {}
        }
        let entry =
            self.registry.get_function(name).copied().ok_or_else(|| {
                NativeError::error(format!("Call to undefined function {name}()"))
            })?;
        prepare_arguments(&entry, &mut args, self)?;
        match entry.handler {
            NativeFn::Sync(f) => f(self, &args),
            NativeFn::Async(f) => f(self, args).await,
        }
    }
}
