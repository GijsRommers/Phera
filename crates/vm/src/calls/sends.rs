//! Sends place values in the callee's reserved window, in source evaluation order.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::calls::call_state::CallTarget;
use crate::calls::call_view::CallView;
use crate::calls::prepared_call::CallFlags;
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
        let view = CallView::top(&self.code, &self.call_stack)?;
        let index = view.argument_index()?;
        let parameters = view.parameters();
        let receiving = parameters.receiving(index);
        let reference = receiving.is_some_and(|parameter| parameters.by_reference(parameter))
            || (index == 0 && !matches!(argument, SentArgument::Value(_)) && view.is_extract());
        let value = match (reference, argument) {
            (true, SentArgument::Value(_)) => {
                return Err(NativeError::error(format!(
                    "{}(): Argument #{} (${}) could not be passed by reference",
                    view.name(),
                    index + 1,
                    receiving.map_or("", |parameter| parameters.name(parameter)),
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

    /// Put an argument at position `index` of the innermost prepared call,
    /// after the checks a named argument needs. A value its parameter's type
    /// does not accept as it is waits for `Call` to be checked.
    pub(crate) fn store_sent_value(
        &mut self,
        index: usize,
        mut value: Value,
    ) -> Result<(), NativeError> {
        let view = CallView::top(&self.code, &self.call_stack)?;
        let slot = view.slots().slot(index);
        let parameters = view.parameters();
        let named = view.details.and_then(|details| details.named.clone());
        let mut extra = false;
        if let Some(name) = &named {
            extra = parameters.position_of(name.as_bytes()).is_none();
            let duplicate = match extra {
                true => view
                    .details
                    .is_some_and(|details| details.extra_named.iter().any(|(_, key)| key == name)),
                false => self
                    .registers
                    .window(slot, 1)
                    .first()
                    .is_some_and(Option::is_some),
            };
            if duplicate {
                return Err(NativeError::error(format!(
                    "Named parameter ${} overwrites previous argument",
                    name.to_string_lossy()
                )));
            }
        }
        let mut deferred = false;
        if let Some(declaration) = parameters
            .receiving(index)
            .and_then(|parameter| parameters.ty(parameter))
            && !crate::types::accepts_as_is(&value, declaration)
        {
            match crate::types::widen_int_to_float(&value, declaration.mask) {
                Some(float) => value = float,
                None => deferred = true,
            }
        }
        let was_named = named.is_some();
        self.registers.grow_to(slot + 1);
        self.registers.put(slot, value);
        if let Some(name) = named {
            let details = self.call_stack.top_details_mut()?;
            details.named = None;
            if extra {
                details.extra_named.push((index, name));
            }
        }
        let call = self.call_stack.top_call_mut()?;
        call.mark_sent(index, !extra);
        if deferred {
            call.flags.insert(CallFlags::DEFERRED_CHECKS);
        }
        if was_named {
            call.flags.insert(CallFlags::NAMED);
        }
        Ok(())
    }
}

impl crate::calls::call_state::Callee {
    /// The PHP function a call enters, for functions and PHP methods.
    pub(crate) fn user_function(&self) -> Option<crate::code_table::FunctionIndex> {
        match &self.target {
            CallTarget::User(function) => Some(*function),
            CallTarget::Method { method, .. } => match method.body {
                crate::classes::MethodBody::User(function) => Some(function),
                _ => None,
            },
            _ => None,
        }
    }

    /// The PHP function a call enters, when its target is PHP code: a
    /// function, a method written in PHP, or a closure over a PHP function.
    pub(crate) fn php_function(&self) -> Option<crate::code_table::FunctionIndex> {
        match &self.target {
            CallTarget::Closure(object) => object
                .native
                .borrow()
                .as_ref()?
                .downcast_ref::<crate::closures::ClosureState>()?
                .callee
                .user_function(),
            _ => self.user_function(),
        }
    }
}
