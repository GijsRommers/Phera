//! Calls are resolved before arguments are evaluated, preserving PHP warning order.

use phera_bytecode::{NameId, PathId, Reg, SymbolId};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::calls::call_state::{CallTarget, Callee, PendingCall};
use crate::{AccessPath, Vm};

impl Vm {
    /// Resolve the callee before argument evaluation so reference parameters are known.
    pub(crate) fn init_call(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("call frame"))?;
        let unit_index = self.code.functions[frame.function.0 as usize].unit;
        let linked = self.code.units[unit_index].calls.borrow()[symbol.index()].clone();
        let callee = match linked {
            Some(callee) => callee,
            None => {
                let definition = &self.code.units[unit_index].unit.symbols[symbol.index()];
                let callee = self
                    .function_table
                    .callee(&definition.name)
                    .or_else(|| {
                        definition
                            .fallback
                            .as_ref()
                            .and_then(|name| self.function_table.callee(name))
                    })
                    .ok_or_else(|| {
                        NativeError::error(format!(
                            "Call to undefined function {}()",
                            definition.written
                        ))
                    })?;
                self.code.units[unit_index].calls.borrow_mut()[symbol.index()] =
                    Some(callee.clone());
                callee
            }
        };
        self.prepare_call(callee)
    }

    /// Resolve a computed callable after publishing its conversion diagnostics.
    pub(crate) fn init_dynamic_call(&mut self, callee: Reg) -> Result<(), NativeError> {
        let value = self.read_local(callee);
        let usage = if self.current().is_some_and(|frame| {
            matches!(
                frame.function(&self.code).instructions.get(frame.ip),
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
    pub(crate) fn resolve_callee(&mut self, name: &str) -> Result<Callee, NativeError> {
        self.function_table
            .callee(name)
            .ok_or_else(|| NativeError::error(format!("Call to undefined function {name}()")))
    }

    /// Resolution reserves the callee window before any argument expression executes.
    pub(crate) fn prepare_call(&mut self, callee: Callee) -> Result<(), NativeError> {
        let registers = match &callee.target {
            CallTarget::User { function } => self.code.function(*function).registers as usize,
            CallTarget::Method { method, .. } => method.function.map_or(
                self.signatures.get(callee.signature).parameters.len(),
                |function| self.code.function(function).registers as usize,
            ),
            CallTarget::Closure(object) => {
                let native = object.native.borrow();
                let state = native
                    .as_ref()
                    .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>())
                    .ok_or_else(|| internal_error!("closure state"))?;
                state.callee.user_function().map_or(
                    self.signatures.get(callee.signature).parameters.len(),
                    |function| self.code.function(function).registers as usize,
                )
            }
            _ => self.signatures.get(callee.signature).parameters.len(),
        }
        .max(self.signatures.get(callee.signature).parameters.len());
        let base = self.registers.len();
        self.registers.grow_to(base + registers);
        let parameters = self.signatures.get(callee.signature).parameters.len();
        self.call_stack.preparing.push(PendingCall {
            owner: None,
            callee,
            window: crate::calls::call_state::CallWindow {
                base,
                registers,
                parameters,
                supplied: 0,
                argument_count: 0,
                checks: crate::calls::call_state::TypeChecks::Complete,
                named: None,
                extra_named: Vec::new(),
                order: crate::calls::call_state::ArgumentOrder::Positional,
            },
        });
        Ok(())
    }

    /// The following send routes to a named parameter rather than the next position.
    pub(crate) fn name_argument(&mut self, name: NameId) -> Result<(), NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("argument without frame"))?;
        let name = frame
            .cache(&self.code)
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
            frame.unit(&self.code).clone(),
            frame.function_id(&self.code),
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
