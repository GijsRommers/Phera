//! Calls are resolved before arguments are evaluated, preserving PHP warning order.

use phera_bytecode::{NameId, PathId, Reg, SymbolId};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::calls::call_state::{CallTarget, Callee, ParameterSpec, PendingCall};
use crate::{AccessPath, Vm};

impl Vm {
    /// Resolve the callee before argument evaluation so reference parameters are known.
    pub(crate) fn init_call(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let cached = self.current().and_then(|frame| {
            frame
                .cache()
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
                .and_then(|frame| frame.unit().symbols.get(symbol.index()))
                .cloned()
                .ok_or_else(|| internal_error!("invalid symbol"))?;
            let name = if self.exists(&definition.name) {
                definition.name
            } else if let Some(fallback) = definition.fallback.filter(|name| self.exists(name)) {
                fallback
            } else {
                definition.written
            };
            let callee = self.resolve_callee(&name)?;
            if let Some(frame) = self.current()
                && let Some(slot) = frame.cache().symbols.borrow_mut().get_mut(symbol.index())
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
                frame.function().instructions.get(frame.ip()),
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
            && let Some(call) = self.call_stack.preparing.last_mut()
        {
            call.owner = Some(object);
        }
        Ok(())
    }

    /// Snapshot the signature and target so argument evaluation cannot change resolution.
    pub(crate) fn resolve_callee(&self, name: &str) -> Result<Callee, NativeError> {
        let lower = name.trim_start_matches('\\').to_ascii_lowercase();
        if let Some(index) = self.function_table.get(&lower) {
            return Ok(self.function_callee(index));
        }
        let (name, target, parameters): (_, _, Vec<_>) =
            if let Some(entry) = self.registry.get_function(name.trim_start_matches('\\')) {
                (
                    entry.name.into(),
                    CallTarget::Native(*entry),
                    entry.params.iter().map(ParameterSpec::native).collect(),
                )
            } else if let Some(intrinsic) = crate::intrinsics::Intrinsic::resolve(name) {
                let parameters = intrinsic
                    .parameter_names()
                    .iter()
                    .map(|name| ParameterSpec {
                        name: (*name).to_owned(),
                        ty: None,
                        by_reference: matches!(intrinsic, crate::intrinsics::Intrinsic::IsCallable)
                            && *name == "callable_name",
                        variadic: matches!(intrinsic, crate::intrinsics::Intrinsic::CallUserFunc)
                            && *name == "args",
                    })
                    .collect();
                (lower.into(), CallTarget::Intrinsic(intrinsic), parameters)
            } else {
                return Err(NativeError::error(format!(
                    "Call to undefined function {name}()"
                )));
            };
        Ok(Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name,
            parameters: parameters.into(),
            target,
        })
    }

    /// The callee record of a declared PHP function.
    pub(crate) fn function_callee(&self, function: crate::code_table::FunctionIndex) -> Callee {
        let unit = self.code.unit(function);
        let body = self.code.function(function);
        Callee {
            argument_binding: crate::calls::callback_binding::ArgumentBinding::Caller,
            name: body.name.as_str().into(),
            parameters: body
                .parameters
                .iter()
                .map(|parameter| ParameterSpec::user(unit, parameter))
                .collect(),
            target: CallTarget::User(function),
        }
    }

    /// Resolution reserves the callee window before any argument expression executes.
    pub(crate) fn prepare_call(&mut self, callee: Callee) -> Result<(), NativeError> {
        let registers = window_registers(&self.code, &callee)?;
        push_prepared(&mut self.call_stack, &mut self.registers, callee, registers);
        Ok(())
    }

    /// The following send routes to a named parameter rather than the next position.
    pub(crate) fn name_argument(&mut self, name: NameId) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("argument without frame"))?;
        let name = frame
            .cache()
            .names
            .get(name.index())
            .cloned()
            .ok_or_else(|| internal_error!("argument name"))?;
        let call = self.call_stack.pending_mut()?;
        call.window.named = Some(name);
        Ok(())
    }

    /// Retain the compiled path while mutable place evaluation borrows the VM.
    pub(crate) fn path(&self, index: PathId) -> Result<AccessPath, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("missing path frame"))?;
        Ok(AccessPath::compiled(
            frame.unit().clone(),
            frame.function_id(),
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

/// How many registers the window of a call to `callee` needs: the callee's
/// own registers for PHP code, one per parameter for native code, and at
/// least one, so that every prepared call starts above the previous one
/// (unwinding tells them apart by where they start).
pub(crate) fn window_registers(
    code: &crate::code_table::CodeTable,
    callee: &Callee,
) -> Result<usize, NativeError> {
    let registers = match &callee.target {
        CallTarget::Closure(object) => {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure state"))?;
            state.callee.user_function()
        }
        _ => callee.user_function(),
    }
    .map_or(0, |function| code.facts(function).registers as usize);
    Ok(registers.max(callee.parameters.len()).max(1))
}

/// Reserve the window at the top of the register stack and record the call.
pub(crate) fn push_prepared(
    stack: &mut crate::calls::call_stack::CallStack,
    registers: &mut crate::calls::registers::Registers,
    callee: Callee,
    size: usize,
) {
    let base = registers.len();
    registers.grow_to(base + size);
    let parameters = callee.parameters.len();
    stack.preparing.push(PendingCall {
        owner: None,
        callee,
        window: crate::calls::call_state::CallWindow {
            base,
            registers: size,
            parameters,
            supplied: 0,
            argument_count: 0,
            checks: crate::calls::call_state::TypeChecks::Complete,
            named: None,
            extra_named: Vec::new(),
            order: crate::calls::call_state::ArgumentOrder::Positional,
        },
    });
}
