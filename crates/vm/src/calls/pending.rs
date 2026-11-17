//! `InitCall` and `InitDynamicCall`: find the callee, then reserve its window.
//!
//! ```php
//! greet($name);      // InitCall: the symbol `greet`, linked once per file
//! $handler($event);  // InitDynamicCall: whatever callable the value is
//! ```
//!
//! The callee is found before the arguments are evaluated, because only it
//! knows which parameters take references, and so that "undefined function"
//! comes before any warning an argument raises, as in PHP. Then the window
//! is reserved at the top of the register stack and a [`PreparedCall`] is
//! pushed. A call to PHP code needs nothing else; any other callee keeps
//! its full [`Callee`] in the call's details.

use phera_bytecode::{NameId, PathId, Reg, SymbolId};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Value, to_php_string};

use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::calls::callback_binding::ArgumentBinding;
use crate::calls::prepared_call::{CallDetails, PreparedCall, PreparedCallee};
use crate::code_table::{CodeTable, FunctionIndex};
use crate::{AccessPath, Vm};

/// What a function symbol of a file was linked to: a PHP function, or the
/// description of anything else (a native function, an intrinsic).
#[derive(Clone)]
pub(crate) enum LinkedFunction {
    Php(FunctionIndex),
    Other(Callee),
}

impl Vm {
    /// Resolve the callee before argument evaluation so reference parameters are known.
    pub(crate) fn init_call(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let linked = self.current().and_then(|frame| {
            frame
                .cache()
                .symbols
                .borrow()
                .get(symbol.index())
                .cloned()
                .flatten()
        });
        let linked = match linked {
            Some(linked) => linked,
            None => self.link_function_symbol(symbol)?,
        };
        match linked {
            LinkedFunction::Php(function) => {
                self.prepare_php_call(function);
                Ok(())
            }
            LinkedFunction::Other(callee) => self.prepare_call(callee),
        }
    }

    /// Find what a function symbol names, with PHP's namespace fallback, and
    /// remember it for the file: a declared function cannot go away.
    fn link_function_symbol(&mut self, symbol: SymbolId) -> Result<LinkedFunction, NativeError> {
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
        let linked = match callee.target {
            CallTarget::User(function) => LinkedFunction::Php(function),
            _ => LinkedFunction::Other(callee),
        };
        if let Some(frame) = self.current()
            && let Some(slot) = frame.cache().symbols.borrow_mut().get_mut(symbol.index())
        {
            *slot = Some(linked.clone());
        }
        Ok(linked)
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
            && self.call_stack.top_details().is_some()
        {
            self.call_stack.top_details_mut()?.owner = Some(object);
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

    /// Reserve the window of a call to a PHP function and record the call.
    #[inline]
    pub(crate) fn prepare_php_call(&mut self, function: FunctionIndex) {
        let base = self.registers.len();
        self.registers
            .grow_to(base + php_window(&self.code, function));
        let call = PreparedCall::new(PreparedCallee::Php(function), base);
        self.call_stack.push_call(call, None);
    }

    /// Reserve the window of a call to `callee` and record the call. A plain
    /// PHP function needs only the record; anything else keeps its
    /// description in the call's details.
    pub(crate) fn prepare_call(&mut self, callee: Callee) -> Result<(), NativeError> {
        if let CallTarget::User(function) = callee.target
            && matches!(callee.argument_binding, ArgumentBinding::Caller)
        {
            self.prepare_php_call(function);
            return Ok(());
        }
        prepare_general(
            &self.code,
            &mut self.call_stack,
            &mut self.registers,
            callee,
        )
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
        self.call_stack.top_details_mut()?.named = Some(name);
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

/// How many registers the window of a call to the PHP function `function`
/// has before any surplus arguments: its own registers, and at least one, so
/// that every prepared call starts above the previous one (unwinding tells
/// them apart by where they start).
#[inline]
pub(crate) fn php_window(code: &CodeTable, function: FunctionIndex) -> usize {
    let facts = code.facts(function);
    (facts.registers.max(facts.parameters) as usize).max(1)
}

/// Reserve the window of a call to a general callee and record the call,
/// with the callee in its details.
pub(crate) fn prepare_general(
    code: &CodeTable,
    stack: &mut crate::calls::call_stack::CallStack,
    registers: &mut crate::calls::registers::Registers,
    callee: Callee,
) -> Result<(), NativeError> {
    let size = general_window(code, &callee)?;
    let base = registers.len();
    registers.grow_to(base + size);
    let details = CallDetails {
        callee: Some(callee),
        ..CallDetails::default()
    };
    stack.push_call(
        PreparedCall::new(PreparedCallee::General, base),
        Some(details),
    );
    Ok(())
}

/// The window size of a call to a general callee: the registers of the PHP
/// function it runs, if any, else one per parameter.
pub(crate) fn general_window(code: &CodeTable, callee: &Callee) -> Result<usize, NativeError> {
    let function = match &callee.target {
        CallTarget::Closure(object) => {
            let native = object.native.borrow();
            let state = native
                .as_ref()
                .and_then(|value| value.downcast_ref::<crate::closures::ClosureState>())
                .ok_or_else(|| internal_error!("closure state"))?;
            state.callee.user_function()
        }
        _ => callee.user_function(),
    };
    let registers = function.map_or(0, |function| php_window(code, function));
    Ok(registers.max(callee.parameters.len()).max(1))
}
