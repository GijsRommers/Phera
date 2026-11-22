//! Sends place values in the callee's reserved window, in source evaluation order.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{PhpString, Value};

use crate::calls::call_state::CallTarget;
use crate::calls::call_view::CallView;
use crate::calls::prepared_call::CallFlags;
use crate::{AccessPath, Vm};

/// Computed values cannot bind aliases; locals and paths preserve their writable origin.
pub(crate) enum SentArgument {
    Value(Reg),
    /// A literal of the file (`SendConst`), sent as a value.
    Literal(Value),
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
            || (index == 0
                && !matches!(argument, SentArgument::Value(_) | SentArgument::Literal(_))
                && view.is_extract());
        let value = match (reference, argument) {
            (true, SentArgument::Value(_) | SentArgument::Literal(_))
                if receiving.is_some_and(|parameter| parameters.requires_reference(parameter)) =>
            {
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
            (_, SentArgument::Value(source)) => self.get(source),
            (_, SentArgument::Literal(value)) => value,
        };
        self.store_sent_value(index, value)
    }

    /// Borrow the cached ordinary property before entering the magic send path.
    pub(crate) fn send_path(&mut self, place: AccessPath) -> Result<bool, NativeError> {
        if !place.has_properties()? {
            self.send(SentArgument::Path(place))?;
            return Ok(true);
        }
        let view = CallView::top(&self.code, &self.call_stack)?;
        let index = view.argument_index()?;
        let parameters = view.parameters();
        let reference = parameters
            .receiving(index)
            .is_some_and(|parameter| parameters.by_reference(parameter))
            || (index == 0 && view.is_extract());
        if !reference && let Some(value) = self.read_property_site(&place)? {
            self.store_sent_value(index, value)?;
            return Ok(true);
        }
        Ok(false)
    }

    /// A property send can run __get, retaining its return reference if needed.
    pub(crate) async fn send_magic_place(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        let view = CallView::top(&self.code, &self.call_stack)?;
        let index = view.argument_index()?;
        let parameters = view.parameters();
        let reference = parameters
            .receiving(index)
            .is_some_and(|parameter| parameters.by_reference(parameter))
            || (index == 0 && view.is_extract());
        if reference {
            self.reference_property_path(
                place.clone(),
                crate::property_hook_continuation::ReferenceCompletion::Argument(index),
            )
            .await
        } else {
            self.read_property_path(
                place.clone(),
                phera_bytecode::FetchMode::Read,
                crate::property_hook_continuation::ReadCompletion::Argument(index),
            )
            .await
        }
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
        let parameters = view.parameters();
        // Declared parameter slots are reserved at the start of the window.
        let slot = if index < parameters.len() {
            view.call.base() + index
        } else {
            view.slots().slot(index)
        };
        let named = view.details.and_then(|details| details.named.as_ref());
        let extra = match named {
            Some(name) => self.check_named_send(&view, slot, name)?,
            None => false,
        };
        let mut deferred = None;
        if let Some(declaration) = parameters
            .receiving(index)
            .and_then(|parameter| parameters.ty(parameter))
            && !crate::types::accepts_as_is(&value, declaration)
        {
            match crate::types::widen_int_to_float(&value, declaration.mask) {
                Some(float) => value = float,
                None => {
                    let string_argument = declaration
                        .mask
                        .intersects(phera_bytecode::TypeMask::STRING)
                        && value.is_object();
                    deferred = Some(string_argument);
                }
            }
        }
        let was_named = named.is_some();
        self.registers.grow_to(slot + 1);
        self.registers.put(slot, value);
        if was_named {
            self.finish_named_send(index, extra)?;
        }
        let call = self.call_stack.top_call_mut()?;
        call.mark_sent(index, !extra);
        if let Some(string_argument) = deferred {
            call.flags.insert(CallFlags::DEFERRED_CHECKS);
            if string_argument {
                call.flags.insert(CallFlags::STRING_ARGUMENT);
            }
        }
        if was_named {
            call.flags.insert(CallFlags::NAMED);
        }
        Ok(())
    }

    /// Named sends may overwrite a parameter or a previous variadic key.
    #[cold]
    #[inline(never)]
    fn check_named_send(
        &self,
        view: &CallView<'_>,
        slot: usize,
        name: &PhpString,
    ) -> Result<bool, NativeError> {
        let extra = view.parameters().position_of(name.as_bytes()).is_none();
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
        Ok(extra)
    }

    /// Move a variadic name into the window after the corresponding value is ready.
    #[cold]
    #[inline(never)]
    fn finish_named_send(&mut self, index: usize, extra: bool) -> Result<(), NativeError> {
        let details = self.call_stack.top_details_mut()?;
        let name = details.named.take();
        if extra && let Some(name) = name {
            details.extra_named.push((index, name));
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
}
