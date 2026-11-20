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
use phera_value::Value;

use crate::calls::call_state::{CallTarget, Callee, ParameterSpec};
use crate::calls::prepared_call::{CallDetails, CallKind, PreparedCall, PreparedCallee};
use crate::code_table::{CodeTable, FunctionIndex};
use crate::{AccessPath, Vm};

/// What a function symbol of a file was linked to: a PHP function, or the
/// description of anything else (a native function, an intrinsic).
#[derive(Clone)]
pub(crate) enum LinkedFunction {
    Php(FunctionIndex),
    Other(Callee),
    /// An unqualified name in a namespace links separately at each instruction.
    NamespaceFallback,
}

impl crate::snapshot::ForkCopy for LinkedFunction {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        match self {
            LinkedFunction::Other(callee) => LinkedFunction::Other(callee.fork_copy(fork)),
            LinkedFunction::Php(_) | LinkedFunction::NamespaceFallback => self.clone(),
        }
    }
}

impl Vm {
    /// Resolve the callee before argument evaluation so reference parameters are known.
    pub(crate) fn init_call(&mut self, symbol: SymbolId) -> Result<(), NativeError> {
        let linked = match self.linked_function(symbol)? {
            LinkedFunction::NamespaceFallback => self.link_namespace_call_site(symbol)?,
            linked => linked,
        };
        match linked {
            LinkedFunction::Php(function) => {
                self.prepare_php_call(function);
                Ok(())
            }
            LinkedFunction::Other(callee) => self.prepare_call(callee),
            LinkedFunction::NamespaceFallback => Err(internal_error!("namespace site target")),
        }
    }

    pub(super) fn linked_function(
        &mut self,
        symbol: SymbolId,
    ) -> Result<LinkedFunction, NativeError> {
        let linked = self.current().and_then(|frame| {
            frame
                .cache()
                .symbols
                .borrow()
                .get(symbol.index())
                .cloned()
                .flatten()
        });
        match linked {
            Some(linked) => Ok(linked),
            None => self.link_function_symbol(symbol),
        }
    }

    /// Find what a function symbol names, with PHP's namespace fallback, and
    /// remember it for the file: a declared function cannot go away.
    fn link_function_symbol(&mut self, symbol: SymbolId) -> Result<LinkedFunction, NativeError> {
        let definition = self.function_symbol(symbol)?;
        let linked = if definition.fallback.is_some() {
            LinkedFunction::NamespaceFallback
        } else {
            self.resolve_function_symbol(&definition)?
        };
        if let Some(frame) = self.current()
            && let Some(slot) = frame.cache().symbols.borrow_mut().get_mut(symbol.index())
        {
            *slot = Some(linked.clone());
        }
        Ok(linked)
    }

    fn function_symbol(&self, symbol: SymbolId) -> Result<phera_bytecode::Symbol, NativeError> {
        self.current()
            .and_then(|frame| frame.unit().symbols.get(symbol.index()))
            .cloned()
            .ok_or_else(|| internal_error!("invalid symbol"))
    }

    /// Zend remembers the first successful target at an INIT_NS_FCALL site.
    /// A different site can see a declaration that happened in the meantime.
    #[inline(never)]
    fn link_namespace_call_site(&self, symbol: SymbolId) -> Result<LinkedFunction, NativeError> {
        use crate::unit_caches::CachedSite;
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("namespace call frame"))?;
        let site = frame
            .cache()
            .site(frame.function_id(), frame.ip())
            .ok_or_else(|| internal_error!("namespace call site"))?;
        if let CachedSite::Function(callee) = &*site.borrow() {
            return Ok((**callee).clone());
        }
        let linked = self.resolve_function_symbol(&self.function_symbol(symbol)?)?;
        *site.borrow_mut() = CachedSite::Function(Box::new(linked.clone()));
        Ok(linked)
    }

    fn resolve_function_symbol(
        &self,
        definition: &phera_bytecode::Symbol,
    ) -> Result<LinkedFunction, NativeError> {
        let name = if self.exists(&definition.name) {
            &definition.name
        } else if let Some(fallback) = definition
            .fallback
            .as_ref()
            .filter(|name| self.exists(name))
        {
            fallback
        } else {
            &definition.name
        };
        let callee = self.resolve_named_callee(
            name,
            &definition.written,
            definition.fallback.as_deref().unwrap_or(&definition.name),
        )?;
        Ok(match callee.target {
            CallTarget::User(function) => LinkedFunction::Php(function),
            _ => LinkedFunction::Other(callee),
        })
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
        self.prepare_call(callee)
    }

    /// Snapshot the signature and target so argument evaluation cannot change resolution.
    pub(crate) fn resolve_callee(&self, name: &str) -> Result<Callee, NativeError> {
        self.resolve_named_callee(name, name, name)
    }

    fn resolve_named_callee(
        &self,
        name: &str,
        written: &str,
        policy_name: &str,
    ) -> Result<Callee, NativeError> {
        let lower = name.trim_start_matches('\\').to_ascii_lowercase();
        if let Some(index) = self.function_table.get(&lower) {
            return Ok(self.function_callee(index));
        }
        let (name, target, parameters): (_, _, Vec<_>) = if let Some(entry) =
            self.registry.get_function(name.trim_start_matches('\\'))
        {
            (
                entry.name.into(),
                CallTarget::Native(*entry),
                entry.params.iter().map(ParameterSpec::native).collect(),
            )
        } else if let Some(intrinsic) = crate::intrinsics::Intrinsic::resolve(name) {
            let parameters = if let Some(parameters) = intrinsic.parameters() {
                parameters.iter().map(ParameterSpec::native).collect()
            } else {
                intrinsic
                    .parameter_names()
                    .iter()
                    .map(|name| ParameterSpec {
                        name: (*name).to_owned(),
                        ty: None,
                        by_reference: matches!(intrinsic, crate::intrinsics::Intrinsic::IsCallable)
                            && *name == "callable_name",
                        requires_reference: matches!(
                            intrinsic,
                            crate::intrinsics::Intrinsic::IsCallable
                        ) && *name == "callable_name",
                        variadic: matches!(
                            (intrinsic, *name),
                            (crate::intrinsics::Intrinsic::CallUserFunc, "args")
                                | (crate::intrinsics::Intrinsic::Compact, "var_names")
                        ),
                    })
                    .collect()
            };
            (lower.into(), CallTarget::Intrinsic(intrinsic), parameters)
        } else {
            return Err(crate::unsupported_items::omitted_item(
                phera_source::deprecations::ItemKind::Function,
                policy_name,
            )
            .unwrap_or_else(|| {
                NativeError::error(format!("Call to undefined function {written}()"))
            }));
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
        let (code, stack, registers) = (&self.code, &mut self.call_stack, &mut self.registers);
        crate::calls::php_windows::push_php_call(
            code,
            (stack, registers),
            function,
            CallKind::Function,
            [],
        );
    }

    /// Reserve the window of a call to `callee` and record the call. A call
    /// to PHP code needs only the record; anything else keeps its
    /// description in the call's details.
    pub(crate) fn prepare_call(&mut self, callee: Callee) -> Result<(), NativeError> {
        if let Err(callee) = self.prepare_php_callee(callee) {
            prepare_general(&mut self.call_stack, &mut self.registers, callee);
        }
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
    pub(crate) async fn exit_value(&mut self, register: Reg) -> NativeResult {
        if self.debug_evaluating {
            return Err(NativeError::error("Cannot exit from a debugger expression"));
        }
        let value = self.get(register);
        if value.is_object()
            && (self
                .current()
                .is_some_and(|frame| frame.function().strict_types)
                || !Self::has_string_conversion(&value))
        {
            return Err(NativeError::type_error(format!(
                "exit(): Argument #1 ($status) must be of type string|int, {} given",
                value.type_name(),
            )));
        }
        if matches!(
            value,
            Value::Int(_) | Value::Bool(_) | Value::Null | Value::Float(_)
        ) {
            let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
            diagnostics.vm.status = phera_value::to_int(&value, &mut diagnostics.buffer)? as i32;
        } else {
            let text = self.stringify_value(&value).await?;
            self.out(text.as_bytes());
        }
        self.release_frames_for_exit();
        Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
            crate::request_end::Exit,
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
    stack: &mut crate::calls::call_stack::CallStack,
    registers: &mut crate::calls::registers::Registers,
    callee: Callee,
) {
    let size = general_window(&callee);
    let base = registers.len();
    registers.grow_to(base + size);
    let mut call = PreparedCall::new(PreparedCallee::General, base);
    if matches!(
        callee.target,
        crate::calls::call_state::CallTarget::Magic(_)
    ) {
        call.flags
            .insert(crate::calls::prepared_call::CallFlags::MAGIC);
    }
    let details = CallDetails {
        callee: Some(callee),
        ..CallDetails::default()
    };
    stack.push_call(call, Some(details));
}

/// The window size of a call to a general callee (native code): one
/// register per parameter, and at least one.
pub(crate) fn general_window(callee: &Callee) -> usize {
    callee.parameters.len().max(1)
}
