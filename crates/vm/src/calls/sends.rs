//! Sends place values in the callee's reserved window, in source evaluation order.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::calls::call_state::{CallTarget, TypeChecks};
use crate::{AccessPath, Vm};

/// Computed values cannot bind aliases; locals and paths preserve their writable origin.
pub(crate) enum SentArgument {
    Value(Reg),
    Local(Reg),
    Path(AccessPath),
}

impl Vm {
    /// Resolve writable aliases only when the callee requires one.
    pub(crate) fn send(&mut self, argument: SentArgument) -> Result<(), NativeError> {
        let call = self.call_stack.pending()?;
        let index = call.argument_index(&self.signatures)?;
        let parameter = call.callee.parameter(index, &self.signatures);
        let reference = parameter.is_some_and(|parameter| parameter.by_reference)
            || (index == 0
                && !matches!(argument, SentArgument::Value(_))
                && matches!(
                    call.callee.target,
                    CallTarget::Intrinsic(crate::intrinsics::Intrinsic::Extract)
                ));
        let value = match (reference, argument) {
            (true, SentArgument::Value(_)) => {
                return Err(NativeError::error(format!(
                    "{}(): Argument #{} (${}) could not be passed by reference",
                    self.signatures.get(call.callee.signature).name,
                    index + 1,
                    parameter.map_or("", |parameter| parameter.name.as_str()),
                )));
            }
            (true, SentArgument::Local(local)) => self.reference(&AccessPath::local(local))?,
            (true, SentArgument::Path(place)) => self.reference(&place)?,
            (false, SentArgument::Local(local)) => self.read_local(local),
            (false, SentArgument::Path(place)) => {
                self.read_place(&place, crate::access_mode::Missing::Warn)?
            }
            (false, SentArgument::Value(source)) => self.get(source),
        };
        self.store_sent_value(index, value)
    }

    /// Duplicate detection inspects the actual slot; rare metadata contains names only.
    pub(crate) fn store_sent_value(
        &mut self,
        index: usize,
        mut value: Value,
    ) -> Result<(), NativeError> {
        let call = self
            .call_stack
            .preparing
            .last_mut()
            .ok_or_else(|| internal_error!("call not initialized"))?;
        let slot = call.window.slot(index);
        let mut extra = false;
        if let Some(name) = call.window.named.take() {
            extra = self
                .signatures
                .get(call.callee.signature)
                .parameters
                .iter()
                .all(|parameter| {
                    parameter.variadic || parameter.name.as_bytes() != name.as_bytes()
                });
            call.window.order = crate::calls::call_state::ArgumentOrder::Named;
            let duplicate = if extra {
                call.window.extra_named.iter().any(|(_, key)| key == &name)
            } else {
                self.registers
                    .window(slot, 1)
                    .first()
                    .is_some_and(Option::is_some)
            };
            if duplicate {
                return Err(NativeError::error(format!(
                    "Named parameter ${} overwrites previous argument",
                    name.to_string_lossy()
                )));
            }
            if extra {
                if call.window.extra_named.capacity() == 0 {
                    call.window.extra_named =
                        self.call_stack.argument_names.pop().unwrap_or_default();
                }
                call.window.extra_named.push((index, name));
            }
        }
        let untyped_values = call
            .callee
            .user_function()
            .is_some_and(|function| self.code.functions[function.0 as usize].untyped_values);
        let parameter = if untyped_values {
            None
        } else {
            call.callee.parameter(index, &self.signatures)
        };
        if let Some(declaration) = parameter.and_then(|parameter| parameter.ty.as_ref())
            && !crate::types::accepts_as_is(&value, declaration)
        {
            if let Some(float) = crate::types::widen_int_to_float(&value, declaration.mask) {
                value = float;
            } else {
                call.window.checks = TypeChecks::Deferred;
            }
        }
        self.registers.grow_to(slot + 1);
        self.registers.window_mut(slot, 1)[0] = Some(value);
        call.window.supplied = call.window.supplied.max(index + 1);
        if !extra {
            call.window.argument_count = call.window.argument_count.max(index + 1);
        }
        Ok(())
    }
}

impl crate::calls::call_state::Callee {
    /// Every surplus value uses the variadic declaration when the signature has one.
    pub(crate) fn parameter<'a>(
        &self,
        index: usize,
        signatures: &'a crate::signatures::Signatures,
    ) -> Option<&'a crate::calls::call_state::ParameterSpec> {
        signatures
            .get(self.signature)
            .parameters
            .get(index)
            .or_else(|| {
                signatures
                    .get(self.signature)
                    .parameters
                    .last()
                    .filter(|parameter| parameter.variadic)
            })
    }

    /// User functions and methods expose identical signature and register metadata.
    pub(crate) fn user_function(&self) -> Option<crate::code_table::FunctionIndex> {
        match &self.target {
            CallTarget::User { function } => Some(*function),
            CallTarget::Method { method, .. } => method.function,
            _ => None,
        }
    }
}
