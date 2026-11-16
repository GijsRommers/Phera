//! Return instructions preserve PHP aliases and stop at native callback boundaries.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::{Vm, dispatch::DispatchResult};

/// Retain return-type metadata while coercion mutably borrows the VM.
struct ReturnDeclaration {
    declaration_type: phera_bytecode::Type,
    name: String,
    checking: crate::types::TypeChecking,
}

impl Vm {
    /// Reference returns carry the reference cell rather than its current value.
    pub(crate) fn return_reference(
        &mut self,
        source: Reg,
        stop: usize,
    ) -> Result<DispatchResult, NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        if !value.is_reference() {
            self.diagnostic(
                "Notice",
                "Only variable references should be returned by reference".into(),
            );
        }
        self.return_value(value, stop)
            .map(DispatchResult::from_return)
    }

    /// Ordinary returns dereference the source before releasing its frame.
    pub(crate) fn return_register(
        &mut self,
        source: Reg,
        stop: usize,
    ) -> Result<DispatchResult, NativeError> {
        let value = self.read_local(source);
        self.return_value(value, stop)
            .map(DispatchResult::from_return)
    }

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
    pub(crate) fn return_value(
        &mut self,
        mut value: Value,
        stop: usize,
    ) -> Result<Option<Value>, NativeError> {
        if let Some(ReturnDeclaration {
            declaration_type,
            name,
            checking,
        }) = self.current().and_then(|frame| {
            frame
                .function()
                .return_type
                .map(|declaration_type| ReturnDeclaration {
                    declaration_type: frame.unit.types[declaration_type.index()].clone(),
                    name: frame.function().name.clone(),
                    checking: frame.function().strict_types.into(),
                })
        }) {
            let reference = match &value {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            };
            let checked = crate::types::coerce(value, &declaration_type, checking, self).map_err(
                |error| match error {
                    NativeError::Throw { class, message } => {
                        let message = message
                            .strip_suffix(" given")
                            .map(|message| format!("{message} returned"))
                            .unwrap_or(message);
                        NativeError::throw(class, format!("{name}(): Return value {message}"))
                    }
                    other => other,
                },
            )?;
            value = if let Some(reference) = reference {
                *reference
                    .try_borrow_mut()
                    .map_err(|_| internal_error!("return reference"))? = checked;
                Value::Reference(reference)
            } else {
                checked
            };
        }
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
