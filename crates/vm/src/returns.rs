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
    pub(crate) fn return_null(&mut self, stop: usize) -> Result<DispatchResult, NativeError> {
        self.return_value(Value::Null, stop)
            .map(DispatchResult::from_return)
    }

    /// Calls contribute to cooperative scheduling even without backward jumps.
    pub(crate) async fn execute_call(
        &mut self,
        destination: Reg,
        work: &mut usize,
    ) -> Result<(), NativeError> {
        *work += 1;
        self.finish_call(destination).await
    }
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
            .current()
            .is_some_and(|frame| frame.function().return_type.is_some())
        {
            value = self.check_return_value(value)?;
        }
        self.finish_return_value(value, stop)
    }

    /// Coercion precedes finally, allowing a surrounding catch to handle an invalid return.
    pub(crate) fn check_return_value(&mut self, mut value: Value) -> Result<Value, NativeError> {
        let typed_return = self.current().and_then(|frame| {
            let declaration = frame.function().return_type?;
            if frame
                .unit
                .types
                .get(declaration.index())
                .is_some_and(|declared| crate::types::accepts_as_is(&value, declared))
            {
                return None;
            }
            // Mirrors the already-matching object arm of types::coerce. A plain
            // accepted object needs neither coercion nor an owned declaration;
            // references retain the ordinary cell-write and borrow checks below.
            if matches!(&value, Value::Object(_))
                && self.matches_cached_class_type(&value, declaration, &frame.unit, &frame.cache)
            {
                return None;
            }
            Some(ReturnTypeSite {
                unit: frame.unit.clone(),
                function: frame.function_id,
                declaration,
            })
        });
        if let Some(ReturnTypeSite {
            unit,
            function: function_id,
            declaration: index,
        }) = typed_return
        {
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
            value = if let Some(reference) = reference {
                *reference
                    .try_borrow_mut()
                    .map_err(|_| internal_error!("return reference"))? = checked;
                Value::Reference(reference)
            } else {
                checked
            };
        }
        Ok(value)
    }

    /// A validated result is held across finally without checking aliases a second time.
    #[inline]
    pub(crate) fn finish_return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        if let Some(context) = self.current().and_then(|frame| frame.method.as_ref())
            && context.invocation == crate::classes::Invocation::Constructor
        {
            value = context
                .receiver
                .clone()
                .map(Value::Object)
                .unwrap_or_default();
        }
        self.finish_include_scope()?;
        let frame = self
            .call_stack
            .pop()
            .ok_or_else(|| internal_error!("return without frame"))?;
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
}

impl Vm {
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
