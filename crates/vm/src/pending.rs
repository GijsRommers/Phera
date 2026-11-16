//! Calls are resolved before arguments are evaluated, preserving PHP warning order.

use phera_bytecode::{NameId, PathId, Reg, SymbolId};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::{AccessPath, Vm};

use phera_extension_api::FunctionEntry;
use std::rc::Rc;

/// A signature and target resolved once for a symbol in this request.
pub(crate) struct Callee {
    pub(crate) name: String,
    pub(crate) parameters: Vec<ParameterSpec>,
    pub(crate) target: CallTarget,
}

/// Named argument routing and PHP parameter binding modifiers.
pub(crate) struct ParameterSpec {
    pub(crate) name: String,
    pub(crate) by_reference: bool,
    pub(crate) variadic: bool,
}

/// User bytecode, a native entry, or an intrinsic resolved at the call site.
pub(crate) enum CallTarget {
    User(crate::function_table::FunctionRef),
    Native(FunctionEntry),
    Intrinsic(crate::intrinsics::Intrinsic),
}

/// An unfinished call; nested argument calls push another entry.
pub(crate) struct PendingCall {
    pub(crate) callee: Rc<Callee>,
    pub(crate) arguments: Vec<Option<Value>>,
    pub(crate) named: Option<String>,
}

impl Vm {
    pub(crate) fn init_call(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let cached = self
            .current()
            .and_then(|frame| frame.cache.borrow().get(symbol.index()).cloned().flatten());
        let callee = if let Some(cached) = cached {
            cached
        } else {
            let definition = self
                .current()
                .and_then(|frame| frame.unit.symbols.get(symbol.index()))
                .cloned()
                .ok_or_else(|| NativeError::error("Phera internal error: invalid symbol"))?;
            let name = if self.exists(&definition.name) {
                definition.name
            } else {
                definition.fallback.unwrap_or(definition.name)
            };
            let callee = self.resolve_callee(name)?;
            if let Some(frame) = self.current()
                && let Some(slot) = frame.cache.borrow_mut().get_mut(symbol.index())
            {
                *slot = Some(callee.clone());
            }
            callee
        };
        self.prepare_call(callee)
    }

    pub(crate) fn init_dynamic_call(&mut self, callee: Reg) -> Result<(), NativeError> {
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        let name = to_php_string(&self.operand(callee), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let callee = self.resolve_callee(name)?;
        self.prepare_call(callee)
    }

    fn resolve_callee(&self, name: String) -> Result<Rc<Callee>, NativeError> {
        let lower = name.to_ascii_lowercase();
        let (target, parameters) =
            if let Some(reference) = self.function_table.functions.get(&lower) {
                let unit = &reference.unit;
                let index = reference.index;
                let function = unit
                    .functions
                    .get(index)
                    .ok_or_else(|| NativeError::error("Phera internal error: function table"))?;
                let parameters = function
                    .parameters
                    .iter()
                    .map(|parameter| ParameterSpec {
                        name: unit
                            .names
                            .get(parameter.name.index())
                            .cloned()
                            .unwrap_or_default(),
                        by_reference: parameter.by_reference,
                        variadic: parameter.variadic,
                    })
                    .collect();
                (CallTarget::User(reference.clone()), parameters)
            } else if let Some(entry) = self.registry.get_function(&name) {
                (
                    CallTarget::Native(*entry),
                    entry
                        .params
                        .iter()
                        .map(|parameter| ParameterSpec {
                            name: parameter.name.to_owned(),
                            by_reference: parameter.by_ref,
                            variadic: parameter.variadic,
                        })
                        .collect(),
                )
            } else if let Some(intrinsic) = crate::intrinsics::Intrinsic::resolve(&name) {
                (CallTarget::Intrinsic(intrinsic), Vec::new())
            } else {
                return Err(NativeError::error(format!(
                    "Call to undefined function {name}()"
                )));
            };
        Ok(Rc::new(Callee {
            name,
            parameters,
            target,
        }))
    }

    fn prepare_call(&mut self, callee: Rc<Callee>) -> Result<(), NativeError> {
        let arguments = self.call_stack.argument_buffers.pop().unwrap_or_default();
        let frame = self
            .call_stack
            .frames
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: call without frame"))?;
        frame.calls.push(PendingCall {
            callee,
            arguments,
            named: None,
        });
        Ok(())
    }

    pub(crate) fn name_argument(&mut self, name: NameId) -> Result<(), NativeError> {
        let frame =
            self.call_stack.frames.last_mut().ok_or_else(|| {
                NativeError::error("Phera internal error: argument without frame")
            })?;
        let name = frame
            .unit
            .names
            .get(name.index())
            .cloned()
            .ok_or_else(|| NativeError::error("Phera internal error: argument name"))?;
        let call = frame
            .calls
            .last_mut()
            .ok_or_else(|| NativeError::error("Phera internal error: argument without call"))?;
        call.named = Some(name);
        Ok(())
    }

    pub(crate) fn send(
        &mut self,
        register: Option<Reg>,
        place: Option<AccessPath>,
    ) -> Result<(), NativeError> {
        let call = self
            .current()
            .and_then(|frame| frame.calls.last())
            .ok_or_else(|| NativeError::error("Phera internal error: send without call"))?;
        let callee = call.callee.clone();
        let name = &callee.name;
        let named = call.named.clone();
        let position = call.arguments.len();
        let params = &callee.parameters;
        let index = if let Some(named) = &named {
            params
                .iter()
                .position(|parameter| &parameter.name == named)
                .ok_or_else(|| NativeError::error(format!("Unknown named parameter ${named}")))?
        } else {
            position
        };
        let by_reference = params
            .get(index)
            .or_else(|| params.last().filter(|parameter| parameter.variadic))
            .is_some_and(|parameter| parameter.by_reference);
        let value = if by_reference {
            if let Some(place) = place {
                self.reference(&place)?
            } else {
                return Err(NativeError::error(format!(
                    "{name}(): Argument #{} cannot be passed by reference",
                    index + 1
                )));
            }
        } else if let Some(place) = place {
            self.read_place(&place, false)?
        } else {
            register
                .map(|register| self.get(register))
                .unwrap_or_default()
        };
        let call = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| frame.calls.last_mut())
            .ok_or_else(|| NativeError::error("Phera internal error: lost call"))?;
        if call.arguments.get(index).is_some_and(Option::is_some) {
            return Err(NativeError::error(format!(
                "Named parameter ${} overwrites previous argument",
                named.unwrap_or_default()
            )));
        }
        call.arguments
            .resize_with(call.arguments.len().max(index + 1), || None);
        call.arguments[index] = Some(value);
        call.named = None;
        Ok(())
    }

    pub(crate) fn path(&self, index: PathId) -> Result<AccessPath, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| NativeError::error("Phera internal error: missing path frame"))?;
        Ok(AccessPath::compiled(
            frame.unit.clone(),
            frame.function_id,
            index,
        ))
    }

    pub(crate) async fn finish_call(&mut self, destination: Reg) -> Result<(), NativeError> {
        let call = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| frame.calls.pop())
            .ok_or_else(|| NativeError::error("Phera internal error: call not initialized"))?;
        let arguments = call.arguments;
        match &call.callee.target {
            CallTarget::User(function) => self.push_prepared(
                function.unit.clone(),
                function.index,
                Some(destination),
                arguments,
            )?,
            CallTarget::Native(entry) => {
                let result = self
                    .native_entry(
                        *entry,
                        arguments
                            .into_iter()
                            .map(Option::unwrap_or_default)
                            .collect(),
                    )
                    .await?;
                self.set(destination, result);
            }
            CallTarget::Intrinsic(intrinsic) => {
                let result = self
                    .intrinsic(
                        *intrinsic,
                        arguments
                            .into_iter()
                            .map(Option::unwrap_or_default)
                            .collect(),
                    )
                    .await?;
                self.set(destination, result);
            }
        }
        Ok(())
    }

    pub(crate) fn return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        if let Some((ty, name, strict)) = self.current().and_then(|frame| {
            frame.function().return_type.map(|ty| {
                (
                    frame.unit.types[ty.index()].clone(),
                    frame.function().name.clone(),
                    frame.function().strict_types,
                )
            })
        }) {
            value =
                crate::types::coerce(value, &ty, strict, self).map_err(|error| match error {
                    NativeError::Throw { class, message } => {
                        NativeError::throw(class, format!("{name}(): Return value {message}"))
                    }
                    other => other,
                })?;
        }
        let frame = self
            .call_stack
            .frames
            .pop()
            .ok_or_else(|| NativeError::error("Phera internal error: return without frame"))?;
        self.registers.truncate(frame.base);
        let destination = frame.destination;
        self.call_stack.recycled_frames.push(frame);
        if self.call_stack.frames.len() == stop {
            return Ok(Some(value));
        }
        if let Some(destination) = destination {
            self.set(destination, value);
        }
        Ok(None)
    }

    pub(crate) fn spread(&mut self, register: Reg) -> Result<(), NativeError> {
        let Value::Array(array) = self.get(register) else {
            return Err(NativeError::type_error(
                "Only arrays and Traversables can be unpacked",
            ));
        };
        for (_, value) in array.iter() {
            let call = self
                .call_stack
                .frames
                .last_mut()
                .and_then(|frame| frame.calls.last_mut())
                .ok_or_else(|| NativeError::error("Phera internal error: spread without call"))?;
            call.arguments.push(Some(value.deref().clone()));
        }
        Ok(())
    }

    pub(crate) fn exit_value(&mut self, register: Reg) -> NativeResult {
        let value = self.get(register);
        let mut diagnostics = std::mem::take(&mut self.diagnostic_reporter.operator_diagnostics);
        if matches!(
            value,
            Value::Int(_) | Value::Bool(_) | Value::Null | Value::Float(_)
        ) {
            self.status = phera_value::to_int(&value, &mut diagnostics)? as i32;
        } else {
            let text = to_php_string(&value, &mut diagnostics)?;
            self.out(text.as_bytes());
        }
        self.diagnostics_flush(diagnostics);
        self.call_stack.frames.clear();
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            (),
        ))))
    }
}
