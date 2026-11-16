//! Call preparation, signature handling and VM intrinsics.
use std::{rc::Rc, sync::Arc};

use phera_bytecode::{CompiledUnit, Reg};
use phera_extension_api::{Context, NativeError, NativeFn, NativeResult, prepare_arguments};
use phera_value::{ArrayKey, PhpArray, Value, to_php_string};

use crate::{Vm, intrinsics::Intrinsic, types};
impl Vm {
    pub(crate) fn named_slot(&mut self, name: &str) -> Result<Reg, NativeError> {
        let frame = self
            .call_stack
            .frames
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: no variable frame"))?;
        if let Some(i) = frame.function().locals.iter().position(|candidate| {
            frame
                .unit
                .names
                .get(candidate.index())
                .is_some_and(|value| value == name)
        }) {
            return Ok(Reg(i as u16));
        }
        if let Some(register) = frame.dynamic.get(name) {
            return Ok(*register);
        }
        let register = Reg(u16::try_from(frame.count)
            .map_err(|_| NativeError::error("Phera register limit exceeded"))?);
        self.registers.push(None);
        frame.count += 1;
        frame.dynamic.insert(name.into(), register);
        Ok(register)
    }
    pub(crate) fn exists(&self, name: &str) -> bool {
        self.function_table
            .functions
            .contains_key(&name.to_ascii_lowercase())
            || self.registry.get_function(name).is_some()
            || Intrinsic::resolve(name).is_some()
    }

    pub(crate) fn push_call(
        &mut self,
        unit: Arc<CompiledUnit>,
        id: usize,
        dst: Option<Reg>,
        arguments: Vec<Value>,
    ) -> Result<(), NativeError> {
        self.push_prepared(unit, id, dst, arguments.into_iter().map(Some).collect())
    }
    pub(crate) fn push_prepared(
        &mut self,
        unit: Arc<CompiledUnit>,
        id: usize,
        dst: Option<Reg>,
        mut arguments: Vec<Option<Value>>,
    ) -> Result<(), NativeError> {
        if self.call_stack.frames.len() >= self.frame_limit {
            return Err(NativeError::error(format!(
                "Maximum call stack depth of {} exceeded",
                self.frame_limit
            )));
        }
        let mut frame = self.make_frame(unit, Some(id), dst);
        if frame.function().observes_arguments {
            frame.arguments = arguments
                .iter()
                .cloned()
                .map(Option::unwrap_or_default)
                .collect();
        }
        let code = frame.unit.clone();
        let function = code
            .functions
            .get(id)
            .ok_or_else(|| NativeError::error("Phera internal error: call function"))?;
        let params = &function.parameters;
        for (i, parameter) in params.iter().enumerate() {
            if arguments.get(i).is_none_or(Option::is_none) && parameter.has_default {
                continue;
            }
            let value = if parameter.variadic {
                let mut values = PhpArray::new();
                for (offset, argument) in arguments.iter().enumerate().skip(i) {
                    let value = argument.clone().unwrap_or_default();
                    let value = self.check_argument(
                        value,
                        parameter.ty.map(|ty| &code.types[ty.index()]),
                        &function.name,
                        offset,
                        &code.names[parameter.name.index()],
                    )?;
                    values.push(value)?;
                }
                Value::Array(Rc::new(values))
            } else {
                let value = arguments.get_mut(i).and_then(Option::take).ok_or_else(|| {
                    let required = params.iter().filter(|parameter| !parameter.has_default && !parameter.variadic).count();
                    NativeError::argument_count_error(format!(
                        "Too few arguments to function {}(), {} passed and at least {required} expected",
                        function.name,arguments.len(),
                    ))
                })?;
                if parameter.ty.is_none() {
                    value
                } else {
                    self.check_argument(
                        value,
                        parameter.ty.map(|ty| &code.types[ty.index()]),
                        &function.name,
                        i,
                        &code.names[parameter.name.index()],
                    )?
                }
            };
            frame.set(&mut self.registers, Reg(i as u16), value);
        }
        arguments.clear();
        self.call_stack.argument_buffers.push(arguments);
        self.call_stack.frames.push(frame);
        Ok(())
    }
    /// Coercion of a reference updates its cell so the caller observes it.
    fn check_argument(
        &mut self,
        value: Value,
        ty: Option<&phera_bytecode::Type>,
        function: &str,
        index: usize,
        parameter: &str,
    ) -> NativeResult {
        let Some(ty) = ty else {
            return Ok(value);
        };
        let (file, line) = self.location();
        let result = types::coerce(value.deref().clone(),ty,self.strict_types(),self)
            .map_err(|error| match error {
                NativeError::Throw { class,message } => {
                    let number = index+1;
                    let message = format!("{function}(): Argument #{number} (${parameter}) {message}, called in {file} on line {line}");
                    NativeError::throw(class,message)
                },
                other => other,
            })?;
        if let Value::Reference(reference) = value {
            *reference
                .try_borrow_mut()
                .map_err(|_| NativeError::error("Phera internal error: argument reference"))? =
                result;
            Ok(Value::Reference(reference))
        } else {
            Ok(result)
        }
    }

    pub(crate) async fn native(&mut self, name: &str, arguments: Vec<Value>) -> NativeResult {
        if let Some(intrinsic) = Intrinsic::resolve(name) {
            return self.intrinsic(intrinsic, arguments).await;
        }
        let entry =
            self.registry.get_function(name).copied().ok_or_else(|| {
                NativeError::error(format!("Call to undefined function {name}()"))
            })?;
        self.native_entry(entry, arguments).await
    }

    /// Execute a resolved intrinsic without matching its function name again.
    pub(crate) async fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<Value>,
    ) -> NativeResult {
        let arg = arguments.first().cloned().unwrap_or_default();
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        match intrinsic {
            Intrinsic::IsCallable => {
                let name = to_php_string(&arg, &mut diagnostics)?.to_string_lossy();
                Ok(Value::Bool(self.exists(&name)))
            }
            Intrinsic::CallUserFunc => {
                let rest = arguments.iter().skip(1).cloned().collect();
                return self.call(&arg, rest).await;
            }
            Intrinsic::CallUserFuncArray => {
                let array = arguments.get(1).cloned().unwrap_or_default();
                let Value::Array(array) = array else {
                    return Err(NativeError::type_error(
                        "call_user_func_array(): Argument #2 ($args) must be of type array",
                    ));
                };
                return self.call(&arg, array.values().cloned().collect()).await;
            }
            Intrinsic::GetDefinedVars => {
                let mut array = PhpArray::new();
                if let Some(frame) = self.current() {
                    for (i, name) in frame.function().locals.iter().enumerate() {
                        if let Some(Some(value)) = frame.slots(&self.registers).get(i) {
                            array.insert(
                                ArrayKey::String(
                                    frame
                                        .unit
                                        .names
                                        .get(name.index())
                                        .cloned()
                                        .unwrap_or_default()
                                        .into(),
                                ),
                                value.deref().clone(),
                            );
                        }
                    }
                    for (name, register) in frame.dynamic.iter() {
                        if let Some(Some(value)) =
                            frame.slots(&self.registers).get(register.0 as usize)
                        {
                            array.insert(
                                ArrayKey::String(name.clone().into()),
                                value.deref().clone(),
                            );
                        }
                    }
                }
                Ok(Value::Array(Rc::new(array)))
            }
            Intrinsic::Compact => {
                let mut names: Vec<_> = arguments.into_iter().rev().collect();
                let mut array = PhpArray::new();
                while let Some(value) = names.pop() {
                    match value {
                        Value::Array(values) => names.extend(
                            values
                                .values()
                                .cloned()
                                .collect::<Vec<_>>()
                                .into_iter()
                                .rev(),
                        ),
                        Value::String(string) => {
                            let name = string.to_string_lossy();
                            let register = self.current().and_then(|frame| {
                                frame
                                    .function()
                                    .locals
                                    .iter()
                                    .position(|x| {
                                        frame
                                            .unit
                                            .names
                                            .get(x.index())
                                            .is_some_and(|candidate| candidate == &name)
                                    })
                                    .map(|i| Reg(i as u16))
                                    .or_else(|| frame.dynamic.get(&name).copied())
                            });
                            if let Some(register) = register {
                                array.insert(ArrayKey::String(string), self.get(register));
                            } else {
                                self.warn(format!("compact(): Undefined variable ${name}"));
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Value::Array(Rc::new(array)))
            }
            Intrinsic::Extract => {
                let Value::Array(array) = arg else {
                    return Err(NativeError::type_error(
                        "extract(): Argument #1 ($array) must be of type array",
                    ));
                };
                let mut count = 0;
                for (k, value) in array.iter() {
                    if let ArrayKey::String(name) = k {
                        let name = name.to_string_lossy();
                        if !name.bytes().enumerate().all(|(i, b)| {
                            b == b'_'
                                || b.is_ascii_alphabetic()
                                || b >= 128
                                || (i > 0 && b.is_ascii_digit())
                        }) || name.is_empty()
                        {
                            continue;
                        }
                        let register = self.named_slot(&name)?;
                        self.set(register, value.deref().clone());
                        count += 1;
                    }
                }
                Ok(Value::Int(count))
            }
            Intrinsic::Define => {
                let name = to_php_string(&arg, &mut diagnostics)?.to_string_lossy();
                if self.constants.contains_key(&name) {
                    self.warn(format!("Constant {name} already defined"));
                    return Ok(Value::Bool(false));
                }
                self.constants
                    .insert(name, arguments.get(1).cloned().unwrap_or_default());
                Ok(Value::Bool(true))
            }
            Intrinsic::Defined => Ok(Value::Bool(
                self.constants
                    .contains_key(&to_php_string(&arg, &mut diagnostics)?.to_string_lossy()),
            )),
            Intrinsic::Constant => {
                let name = to_php_string(&arg, &mut diagnostics)?.to_string_lossy();
                self.constants
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| NativeError::error(format!("Undefined constant \"{name}\"")))
            }
            Intrinsic::FunctionExists => Ok(Value::Bool(
                self.exists(&to_php_string(&arg, &mut diagnostics)?.to_string_lossy()),
            )),
            Intrinsic::FuncNumArgs => Ok(Value::Int(
                self.current().map_or(0, |frame| frame.arguments.len()) as i64,
            )),
            Intrinsic::FuncGetArgs => {
                let array = self
                    .current()
                    .map(|frame| frame.arguments.clone())
                    .unwrap_or_default();
                let mut result = PhpArray::new();
                for value in array {
                    result.push(value)?;
                }
                Ok(Value::Array(Rc::new(result)))
            }
            Intrinsic::FuncGetArg => {
                let i = phera_value::to_int(&arg, &mut diagnostics)?;
                self.current().and_then(|frame|frame.arguments.get(i as usize)).cloned().ok_or_else(||NativeError::value_error("func_get_arg(): Argument #1 ($position) must be less than the number of the arguments passed to the currently executed function"))
            }
            Intrinsic::ErrorReporting => Ok(Value::Int(30719)),
            Intrinsic::IniSet | Intrinsic::IniGet => Ok(Value::from("1")),
        }
    }

    pub(crate) async fn native_entry(
        &mut self,
        entry: phera_extension_api::FunctionEntry,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        prepare_arguments(&entry, &mut arguments, self)?;
        match entry.handler {
            NativeFn::Sync(frame) => frame(self, &arguments),
            NativeFn::Async(frame) => frame(self, arguments).await,
        }
    }
}
