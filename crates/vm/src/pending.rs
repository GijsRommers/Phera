//! Calls are resolved before arguments are evaluated, preserving PHP warning order.

use std::rc::Rc;

use phera_bytecode::{NameId, PathId, Reg, SymbolId};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::call_state::{CallTarget, Callee, ParameterSpec, PendingCall, ResolvedSignature};
use crate::{AccessPath, Vm};

impl Vm {
    /// Resolve the callee before argument evaluation so reference parameters are known.
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
                .ok_or_else(|| internal_error!("invalid symbol"))?;
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

    /// Resolve a computed callable after publishing its conversion diagnostics.
    pub(crate) fn init_dynamic_call(&mut self, callee: Reg) -> Result<(), NativeError> {
        let mut diagnostics = self.diagnostic_reporter.take_operator();
        let name = to_php_string(&self.operand(callee), &mut diagnostics)?.to_string_lossy();
        self.diagnostics_flush(diagnostics);
        let callee = self.resolve_callee(name)?;
        self.prepare_call(callee)
    }

    /// Snapshot the signature and target so argument evaluation cannot change resolution.
    fn resolve_callee(&self, name: String) -> Result<Rc<Callee>, NativeError> {
        let lower = name.to_ascii_lowercase();
        let ResolvedSignature { target, parameters } =
            if let Some(reference) = self.function_table.get(&lower) {
                let unit = &reference.unit;
                let index = reference.index;
                let function = unit
                    .functions
                    .get(index)
                    .ok_or_else(|| internal_error!("function table"))?;
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
                ResolvedSignature {
                    target: CallTarget::User(reference.clone()),
                    parameters,
                }
            } else if let Some(entry) = self.registry.get_function(&name) {
                ResolvedSignature {
                    target: CallTarget::Native(*entry),
                    parameters: entry
                        .params
                        .iter()
                        .map(|parameter| ParameterSpec {
                            name: parameter.name.to_owned(),
                            by_reference: parameter.by_ref,
                            variadic: parameter.variadic,
                        })
                        .collect(),
                }
            } else if let Some(intrinsic) = crate::intrinsics::Intrinsic::resolve(&name) {
                ResolvedSignature {
                    target: CallTarget::Intrinsic(intrinsic),
                    parameters: Vec::new(),
                }
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

    /// Nested argument calls keep independent pending calls in the current heap frame.
    fn prepare_call(&mut self, callee: Rc<Callee>) -> Result<(), NativeError> {
        let arguments = self.call_stack.take_arguments();
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("call without frame"))?;
        frame.calls.push(PendingCall {
            callee,
            arguments,
            named: None,
            extra_named: None,
        });
        Ok(())
    }

    /// The following send routes to a named parameter rather than the next position.
    pub(crate) fn name_argument(&mut self, name: NameId) -> Result<(), NativeError> {
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("argument without frame"))?;
        let name = frame
            .unit
            .names
            .get(name.index())
            .cloned()
            .ok_or_else(|| internal_error!("argument name"))?;
        let call = frame
            .calls
            .last_mut()
            .ok_or_else(|| internal_error!("argument without call"))?;
        call.named = Some(name);
        Ok(())
    }

    /// Choose value copying or reference binding from the resolved parameter signature.
    pub(crate) fn send(
        &mut self,
        register: Option<Reg>,
        place: Option<AccessPath>,
    ) -> Result<(), NativeError> {
        let call = self
            .current()
            .and_then(|frame| frame.calls.last())
            .ok_or_else(|| internal_error!("send without call"))?;
        let callee = call.callee.clone();
        let name = &callee.name;
        let named = call.named.clone();
        let parameters = &callee.parameters;
        let index = call.argument_index()?;
        let by_reference = parameters
            .get(index)
            .or_else(|| parameters.last().filter(|parameter| parameter.variadic))
            .is_some_and(|parameter| parameter.by_reference);
        let value = if matches!(
            callee.target,
            CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Extract)
        ) && index == 0
            && place.is_some()
        {
            self.reference(
                place
                    .as_ref()
                    .ok_or_else(|| internal_error!("extract place"))?,
            )?
        } else if by_reference {
            if let Some(place) = place {
                self.reference(&place)?
            } else {
                return Err(NativeError::error(format!(
                    "{name}(): Argument #{} cannot be passed by reference",
                    index + 1
                )));
            }
        } else if let Some(place) = place {
            self.read_place(&place, crate::access_mode::Missing::Warn)?
        } else {
            register
                .map(|register| self.get(register))
                .unwrap_or_default()
        };
        let call = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.last_mut())
            .ok_or_else(|| internal_error!("lost call"))?;
        call.store_argument(index, named, value)?;
        Ok(())
    }

    pub(crate) fn path(&self, index: PathId) -> Result<AccessPath, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("missing path frame"))?;
        Ok(AccessPath::compiled(
            frame.unit.clone(),
            frame.function_id,
            index,
        ))
    }

    /// Push a PHP heap frame or await native code without recursing for PHP calls.
    pub(crate) async fn finish_call(&mut self, destination: Reg) -> Result<(), NativeError> {
        let call = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.pop())
            .ok_or_else(|| internal_error!("call not initialized"))?;
        let mut arguments = call.arguments;
        let mut named_keys = Vec::new();
        let mut named_start = 0;
        if let Some(extra_named) = call.extra_named {
            named_start = arguments
                .len()
                .max(call.callee.parameters.len().saturating_sub(1));
            arguments.resize_with(named_start, || None);
            for argument in extra_named.values {
                named_keys.push(argument.name);
                arguments.push(Some(argument.value));
            }
        }
        match &call.callee.target {
            CallTarget::User(function) => {
                self.push_prepared(
                    function.unit.clone(),
                    function.index,
                    Some(destination),
                    arguments,
                )?;
                self.name_variadic_arguments(named_start, named_keys)?;
            }
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

    /// Numeric exit values set status; strings are output before unwinding the request.
    pub(crate) fn exit_value(&mut self, register: Reg) -> NativeResult {
        let value = self.get(register);
        let mut diagnostics = self.diagnostic_reporter.take_operator();
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
        self.call_stack.clear();
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            (),
        ))))
    }
}
