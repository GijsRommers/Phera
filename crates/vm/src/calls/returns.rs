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
}

/// Retain code metadata while coercion mutably borrows the interpreter.
struct ReturnTypeSite {
    unit: std::sync::Arc<phera_bytecode::CompiledUnit>,
    function: Option<usize>,
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
        if self.current().is_some_and(|frame| {
            self.code.functions[frame.function.0 as usize]
                .return_mask
                .is_some()
        }) {
            value = self.check_return_value(value)?;
        }
        self.finish_return_value(value, stop)
    }

    /// Coercion precedes finally, allowing a surrounding catch to handle an invalid return.
    pub(crate) fn check_return_value(&mut self, value: Value) -> Result<Value, NativeError> {
        if let Some(declaration) = self.current().and_then(|frame| {
            frame
                .function(&self.code)
                .return_type
                .and_then(|ty| frame.unit(&self.code).types.get(ty.index()))
        }) && let Some(float) = crate::types::widen_int_to_float(&value, declaration.mask)
        {
            return Ok(float);
        }
        let typed_return = self.current().and_then(|frame| {
            let declaration = frame.function(&self.code).return_type?;
            if frame
                .unit(&self.code)
                .types
                .get(declaration.index())
                .is_some_and(|declared| crate::types::accepts_as_is(&value, declared))
            {
                return None;
            }
            // A cached class identity can accept an object without cloning its declaration.
            // References still need the ordinary cell-write and borrow checks below.
            if matches!(&value, Value::Object(_))
                && self.matches_cached_class_type(
                    &value,
                    declaration,
                    frame.unit(&self.code),
                    frame.cache(&self.code),
                )
            {
                return None;
            }
            Some(ReturnTypeSite {
                unit: frame.unit(&self.code).clone(),
                function: frame.function_id(&self.code),
                declaration,
            })
        });
        match typed_return {
            Some(site) => self.coerce_return_value(value, site),
            None => Ok(value),
        }
    }

    /// Slow coercion may warn, throw or update a returned PHP reference cell.
    fn coerce_return_value(
        &mut self,
        value: Value,
        site: ReturnTypeSite,
    ) -> Result<Value, NativeError> {
        let ReturnTypeSite {
            unit,
            function: function_id,
            declaration: index,
        } = site;
        let function = function_id
            .and_then(|id| unit.functions.get(id))
            .unwrap_or(&unit.main);
        let declaration_type = &unit.types[index.index()];
        let reference = match &value {
            Value::Reference(reference) => Some(reference.clone()),
            _ => None,
        };
        let checked =
            crate::types::coerce(value, declaration_type, function.strict_types.into(), self)
                .map_err(|error| match error {
                    NativeError::Throw { class, message, .. } => {
                        let message = message
                            .strip_suffix(" given")
                            .map(|message| format!("{message} returned"))
                            .unwrap_or(message);
                        NativeError::throw(
                            class,
                            format!("{}(): Return value {message}", function.name),
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

    /// A validated result is held across finally without checking aliases a second time.
    #[inline]
    pub(crate) fn finish_return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        if let Some(context) = self
            .current()
            .and_then(|frame| self.call_stack.extras[frame.depth].method.as_ref())
            && context.invocation == crate::classes::Invocation::Constructor
        {
            value = self
                .frame_receiver(context)
                .cloned()
                .map(Value::Object)
                .unwrap_or_default();
        }
        self.finish_include_scope()?;
        let frame = self
            .call_stack
            .pop()
            .ok_or_else(|| internal_error!("return without frame"))?;
        self.call_stack.preparing.truncate(frame.call_start);
        self.registers.truncate(frame.base);
        let destination = frame.destination;
        self.call_stack.recycle(frame);
        if self.call_stack.depth() == stop {
            return Ok(Some(value));
        }
        if let Some(destination) = destination {
            self.set(destination, value);
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
                | DispatchedReturn::Checked(source) => source,
            };
            let value = if matches!(kind, DispatchedReturn::Value(_)) {
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
                        .is_some_and(|frame| frame.function(&self.code).returns_reference));
            if reference_return && !value.is_reference() {
                self.diagnostic(
                    "Notice",
                    "Only variable references should be returned by reference".into(),
                );
            }
            if !self.error_handlers.pending.is_empty() {
                self.drain_return_handlers().await?;
            }
            if matches!(kind, DispatchedReturn::Checked(_)) {
                self.finish_return_value(value, stop)
                    .map(DispatchResult::from_return)
            } else {
                self.return_value(value, stop)
                    .map(DispatchResult::from_return)
            }
        })
    }
}
