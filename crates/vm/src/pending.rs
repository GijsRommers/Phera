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
        let cached = self.current().and_then(|frame| {
            frame
                .cache
                .symbols
                .borrow()
                .get(symbol.index())
                .cloned()
                .flatten()
        });
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
            } else if let Some(fallback) = definition.fallback.filter(|name| self.exists(name)) {
                fallback
            } else {
                definition.written
            };
            let callee = self.resolve_callee(name)?;
            if let Some(frame) = self.current()
                && let Some(slot) = frame.cache.symbols.borrow_mut().get_mut(symbol.index())
            {
                *slot = Some(callee.clone());
            }
            callee
        };
        self.prepare_call(callee)
    }

    /// Resolve a computed callable after publishing its conversion diagnostics.
    pub(crate) fn init_dynamic_call(&mut self, callee: Reg) -> Result<(), NativeError> {
        let value = self.read_local(callee);
        let usage = if self.current().is_some_and(|frame| {
            matches!(
                frame.function().instructions.get(frame.ip),
                Some(phera_bytecode::Instruction::MakeCallable { .. })
            )
        }) {
            crate::callables::CallableUse::FirstClass
        } else {
            crate::callables::CallableUse::Direct
        };
        let callee = self.resolve_callable_for(&value, usage)?;
        self.prepare_call(callee)?;
        if let Value::Object(object) = value
            && object.class.name() == "Closure"
            && let Some(call) = self
                .call_stack
                .current_mut()
                .and_then(|frame| frame.calls.last_mut())
        {
            call.owner = Some(object);
        }
        Ok(())
    }

    /// Snapshot the signature and target so argument evaluation cannot change resolution.
    pub(crate) fn resolve_callee(&self, name: String) -> Result<Rc<Callee>, NativeError> {
        let lower = name.trim_start_matches('\\').to_ascii_lowercase();
        let ResolvedSignature {
            name,
            target,
            parameters,
        } = if let Some(reference) = self.function_table.get(&lower) {
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
                name: function.name.as_str().into(),
                target: CallTarget::User(reference.clone()),
                parameters,
            }
        } else if let Some(entry) = self.registry.get_function(name.trim_start_matches('\\')) {
            ResolvedSignature {
                name: entry.name.into(),
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
                name: lower.into(),
                target: CallTarget::Intrinsic(intrinsic),
                parameters: intrinsic
                    .parameter_names()
                    .iter()
                    .map(|name| ParameterSpec {
                        name: (*name).to_owned(),
                        by_reference: matches!(intrinsic, crate::intrinsics::Intrinsic::IsCallable)
                            && *name == "callable_name",
                        variadic: matches!(intrinsic, crate::intrinsics::Intrinsic::CallUserFunc)
                            && *name == "args",
                    })
                    .collect(),
            }
        } else {
            return Err(NativeError::error(format!(
                "Call to undefined function {name}()"
            )));
        };
        Ok(Rc::new(Callee {
            argument_binding: crate::callback_binding::ArgumentBinding::Caller,
            name,
            parameters: parameters.into(),
            target,
        }))
    }

    /// Nested argument calls keep independent pending calls in the current heap frame.
    pub(crate) fn prepare_call(&mut self, callee: Rc<Callee>) -> Result<(), NativeError> {
        let arguments = self.call_stack.take_arguments();
        let frame = self
            .call_stack
            .current_mut()
            .ok_or_else(|| internal_error!("call without frame"))?;
        frame.calls.push(PendingCall {
            owner: None,
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
                    "{name}(): Argument #{} (${}) could not be passed by reference",
                    index + 1,
                    callee
                        .parameters
                        .get(index)
                        .map_or("", |parameter| parameter.name.as_str())
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

    /// Numeric exit values set status; strings are output before unwinding the request.
    pub(crate) fn exit_value(&mut self, register: Reg) -> NativeResult {
        let value = self.get(register);
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        if matches!(
            value,
            Value::Int(_) | Value::Bool(_) | Value::Null | Value::Float(_)
        ) {
            diagnostics.vm.status = phera_value::to_int(&value, &mut diagnostics.buffer)? as i32;
        } else {
            let text = to_php_string(&value, &mut diagnostics.buffer)?;
            diagnostics.vm.out(text.as_bytes());
        }
        drop(diagnostics);
        self.call_stack.clear();
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            (),
        ))))
    }
}
