//! Return instructions preserve PHP aliases and stop at native callback boundaries.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, dispatch::DispatchResult};

/// Cold return variants keep handler futures out of the ordinary instruction dispatcher.
pub(crate) enum DispatchedReturn {
    Value(Reg),
    Reference(Reg),
    Checked(Reg),
    Generator(Reg),
}

/// Retain code metadata while coercion mutably borrows the interpreter.
struct ReturnTypeSite {
    unit: std::sync::Arc<phera_bytecode::CompiledUnit>,
    function: crate::code_table::FunctionIndex,
    declaration: phera_bytecode::TypeId,
}

impl Vm {
    /// Check the declared result before releasing registers and recycling the callee.
    #[inline]
    pub(crate) fn return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        if self
            .call_stack
            .current()
            .is_some_and(|frame| self.code.facts(frame.function).return_mask.is_some())
        {
            value = self.check_return_value(value)?;
        }
        self.finish_return_value(value, stop)
    }

    pub(crate) fn return_string_coercion_needed(&self, value: &Value) -> bool {
        self.current().is_some_and(|frame| {
            let function = frame.function();
            function
                .return_type
                .and_then(|ty| frame.unit().types.get(ty.index()))
                .is_some_and(|ty| {
                    self.needs_string_coercion(value, ty, function.strict_types.into())
                })
        })
    }

    pub(crate) fn missing_magic_return(&self) -> NativeError {
        let Some(frame) = self.current() else {
            return internal_error!("missing magic return frame");
        };
        let function = frame.function();
        let declared = function
            .return_type
            .and_then(|id| frame.unit().types.get(id.index()));
        if declared.is_some_and(|ty| ty.mask.intersects(phera_bytecode::TypeMask::NEVER)) {
            NativeError::type_error(format!(
                "{}(): never-returning method must not implicitly return",
                function.name
            ))
        } else {
            NativeError::type_error(format!(
                "{}(): Return value must be of type {}, none returned",
                function.name,
                declared.map_or("string", |ty| ty.display.as_str())
            ))
        }
    }

    /// Coercion precedes finally, allowing a surrounding catch to handle an invalid return.
    pub(crate) fn check_return_value(&mut self, value: Value) -> Result<Value, NativeError> {
        if let Some(declaration) = self.current().and_then(|frame| {
            frame
                .function()
                .return_type
                .and_then(|ty| frame.unit().types.get(ty.index()))
        }) && let Some(float) = crate::types::widen_int_to_float(&value, declaration.mask)
        {
            return Ok(float);
        }
        let mut typed_return = self.unaccepted_return(&value);
        // A fork's file has no class checks until one is asked for: make
        // them from the snapshot's and ask again, so that this value and
        // the next of its class are accepted by the remembered answer.
        if let Some(site) = &typed_return
            && !self.code.cache(site.function).class_types.is_made()
        {
            self.code.make_class_types(site.function);
            typed_return = self.unaccepted_return(&value);
        }
        match typed_return {
            Some(site) => self.coerce_return_value(value, site),
            None => Ok(value),
        }
    }

    /// The return type of the running function, unless `value` satisfies it
    /// as it is: by its built-in type, or by a class the file's class checks
    /// remember.
    fn unaccepted_return(&self, value: &Value) -> Option<ReturnTypeSite> {
        self.current().and_then(|frame| {
            let declaration = frame.function().return_type?;
            if frame
                .unit()
                .types
                .get(declaration.index())
                .is_some_and(|declared| crate::types::accepts_as_is(value, declared))
            {
                return None;
            }
            // A cached class identity can accept an object without cloning its declaration.
            // References still need the ordinary cell-write and borrow checks below.
            if matches!(value, Value::Object(_))
                && self.matches_cached_class_type(value, declaration, frame.unit(), frame.cache())
            {
                return None;
            }
            Some(ReturnTypeSite {
                unit: frame.unit().clone(),
                function: frame.frame.function,
                declaration,
            })
        })
    }

    /// Slow coercion may warn, throw or update a returned PHP reference cell.
    fn coerce_return_value(
        &mut self,
        value: Value,
        site: ReturnTypeSite,
    ) -> Result<Value, NativeError> {
        let ReturnTypeSite {
            unit,
            function,
            declaration: index,
        } = site;
        let strict = self.code.function(function).strict_types;
        let declaration_type = &unit.types[index.index()];
        let scoped = self.current().and_then(|frame| {
            let method = frame.method()?;
            (method.scope.kind == phera_bytecode::ClassKind::Anonymous)
                .then(|| crate::closure_types::parameter_type(declaration_type, &method))
        });
        let declaration_type = scoped.as_ref().unwrap_or(declaration_type);
        let reference = match &value {
            Value::Reference(reference) => Some(reference.clone()),
            _ => None,
        };
        let scope = declaration_type
            .mask
            .intersects(phera_bytecode::TypeMask::CALLABLE)
            .then(|| self.current().and_then(|frame| frame.method()))
            .flatten();
        let checked = crate::types::coerce_in_scope(
            value,
            declaration_type,
            strict.into(),
            self,
            scope.as_ref(),
        )
        .map_err(|error| match error {
            NativeError::Throw { class, message, .. } => {
                let message = message
                    .strip_suffix(" given")
                    .map(|message| format!("{message} returned"))
                    .unwrap_or(message);
                NativeError::throw(
                    class,
                    format!(
                        "{}(): Return value {message}",
                        self.return_function_name(function)
                    ),
                )
            }
            other => other,
        })?;
        if let Some(reference) = reference {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("return reference"))? = checked;
            Ok(Value::Reference(reference))
        } else {
            Ok(checked)
        }
    }

    fn return_function_name(
        &self,
        function: crate::code_table::FunctionIndex,
    ) -> std::borrow::Cow<'_, str> {
        let name = self.code.function_name(function);
        if let Some((short, _)) = name.split_once('\0')
            && let Some((_, method)) = name.rsplit_once("::")
        {
            format!("{short}::{method}").into()
        } else {
            name.into()
        }
    }

    /// A validated result is held across finally without checking aliases a second time.
    #[inline]
    pub(crate) fn finish_return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        let mut frame = self
            .call_stack
            .current()
            .copied()
            .ok_or_else(|| internal_error!("return without frame"))?;
        if frame.kind() == crate::calls::prepared_call::CallKind::Constructor {
            value = self
                .current()
                .and_then(|frame| frame.this().cloned())
                .map(Value::Object)
                .unwrap_or_default();
        }
        if self.code.facts(frame.function).local.is_none() {
            // Includes may move their frame while copying locals back to the caller.
            self.finish_include_scope()?;
            frame = self
                .call_stack
                .current()
                .copied()
                .ok_or_else(|| internal_error!("include return without frame"))?;
        }
        let property_hook = if frame.has_extras()
            && frame.destination().is_none()
            && self.call_stack.depth() > stop + 1
        {
            self.call_stack.current_extras_mut().property_hook.take()
        } else {
            None
        };
        self.call_stack.pop_for_return();
        self.call_stack.abandon_calls_from(frame.start());
        self.registers.release_frame(frame.start(), frame.base());
        if frame.returns_to_native_loop() {
            self.resume_native_loop(value)?;
            return Ok(None);
        }
        if self.call_stack.depth() == stop {
            return Ok(Some(value));
        }
        if let Some(destination) = frame.destination() {
            self.set(destination, value);
        } else if let Some(continuation) = property_hook {
            self.returned_hook = Some(crate::property_hook_continuation::ReturnedHook {
                continuation,
                value,
            });
            self.pending_callbacks = true;
        }
        Ok(None)
    }
    /// Only these cold returns allocate an erased future; ordinary PHP returns stay synchronous.
    pub(crate) fn dispatched_return(
        &mut self,
        kind: DispatchedReturn,
        stop: usize,
    ) -> phera_extension_api::LocalFuture<'_, Result<DispatchResult, NativeError>> {
        Box::pin(async move {
            let source = match kind {
                DispatchedReturn::Value(source)
                | DispatchedReturn::Reference(source)
                | DispatchedReturn::Checked(source)
                | DispatchedReturn::Generator(source) => source,
            };
            let value = if matches!(
                kind,
                DispatchedReturn::Value(_) | DispatchedReturn::Generator(_)
            ) {
                self.read_local(source)
            } else {
                self.current()
                    .and_then(|frame| frame.slots(&self.registers).get(source.index()))
                    .and_then(Option::as_ref)
                    .cloned()
                    .unwrap_or_default()
            };
            let reference_return = matches!(kind, DispatchedReturn::Reference(_))
                || (matches!(kind, DispatchedReturn::Checked(_))
                    && self
                        .current()
                        .is_some_and(|frame| frame.function().returns_reference));
            if reference_return && !value.is_reference() {
                self.diagnostic(
                    "Notice",
                    "Only variable references should be returned by reference".into(),
                );
            }
            if !self.error_handlers.pending.is_empty() {
                self.drain_return_handlers().await?;
            }
            if matches!(
                kind,
                DispatchedReturn::Checked(_) | DispatchedReturn::Generator(_)
            ) {
                self.finish_return_value(value, stop)
                    .map(DispatchResult::from_return)
            } else {
                let value = if self.return_string_coercion_needed(&value) {
                    self.coerce_stringable(value).await?
                } else {
                    value
                };
                self.return_value(value, stop)
                    .map(DispatchResult::from_return)
            }
        })
    }
}
