//! Array callbacks route keys through the same preparation stack as unpacking.

use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray};

use crate::Vm;
use crate::calls::call_state::Callee;
use crate::calls::call_view::CallView;
use crate::calls::callback_binding::ArgumentOrigin;
use crate::calls::prepared_call::CallFlags;

impl Vm {
    /// Callback arrays can explicitly retain caller reference cells.
    pub(crate) fn invoke_array_callback(
        &mut self,
        callee: Callee,
        array: &PhpArray,
    ) -> LocalFuture<'_, NativeResult> {
        self.invoke_array_callback_origin(callee, array, ArgumentOrigin::Callback)
    }

    /// Routing is synchronous, so the PHP array need not be borrowed across the returned future.
    pub(crate) fn invoke_array_callback_origin(
        &mut self,
        callee: Callee,
        array: &PhpArray,
        origin: ArgumentOrigin,
    ) -> LocalFuture<'_, NativeResult> {
        let origin = callee.argument_binding.origin(origin);
        let base = self.registers.len();
        let depth = self.call_stack.call_depth();
        let routed = self
            .prepare_call(callee)
            .and_then(|()| self.route_callback_array(array));
        Box::pin(self.finish_callback(routed, base, depth, origin))
    }

    /// Integer keys follow positional order and string keys select declared or variadic slots.
    pub(crate) fn route_callback_array(&mut self, array: &PhpArray) -> Result<(), NativeError> {
        if array.is_empty() {
            return Ok(());
        }
        let mut call = *self.call_stack.top_call()?;
        let base = call.base();
        let (param_len, window_size) = {
            let view = CallView::new(&self.code, call, self.call_stack.top_details());
            let slots = view.slots();
            let param_len = view.parameters().len();
            (param_len, slots.slot(param_len) - base)
        };
        let slot_for = |index: usize| {
            if index < param_len {
                base + index
            } else {
                base + window_size + index - param_len
            }
        };

        // Fast positional path for plain lists when no named argument has been sent yet.
        if array.is_list() && !call.flags.contains(CallFlags::NAMED) {
            let end_index = call.sent as usize + array.len();
            self.registers.grow_to(slot_for(end_index - 1) + 1);
            let parameters =
                CallView::new(&self.code, call, self.call_stack.top_details()).parameters();
            for value in array.values() {
                let index = call.sent as usize;
                let mut value = value.clone();
                if let Some(declaration) = parameters
                    .receiving(index)
                    .and_then(|parameter| parameters.ty(parameter))
                    && !crate::types::accepts_as_is(&value, declaration)
                {
                    match crate::types::widen_int_to_float(&value, declaration.mask) {
                        Some(float) => value = float,
                        None => {
                            call.flags.insert(CallFlags::DEFERRED_CHECKS);
                            if declaration
                                .mask
                                .intersects(phera_bytecode::TypeMask::STRING)
                                && value.is_object()
                            {
                                call.flags.insert(CallFlags::STRING_ARGUMENT);
                            }
                        }
                    }
                }
                self.registers.put(slot_for(index), value);
                call.mark_sent(index, true);
            }
            *self.call_stack.top_call_mut()? = call;
            return Ok(());
        }

        // Mixed or associative callback arrays: resolve each named key once without
        // round-tripping through `details.named` or scanning `position_of` twice.
        for (key, value) in array.iter() {
            let (index, extra, was_named, named_extra) = match key {
                ArrayKey::Int(_) if call.flags.contains(CallFlags::NAMED) => {
                    return Err(NativeError::error(
                        "Cannot use positional argument after named argument",
                    ));
                }
                ArrayKey::Int(_) => (call.sent as usize, false, false, None),
                ArrayKey::Interned(name) => {
                    let bytes = name.as_bytes();
                    let view = CallView::new(&self.code, call, self.call_stack.top_details());
                    let parameters = view.parameters();
                    if let Some(pos) = parameters.position_of(bytes) {
                        let slot = slot_for(pos);
                        if self
                            .registers
                            .window(slot, 1)
                            .first()
                            .is_some_and(Option::is_some)
                        {
                            return Err(NativeError::error(format!(
                                "Named parameter ${} overwrites previous argument",
                                String::from_utf8_lossy(bytes)
                            )));
                        }
                        (pos, false, true, None)
                    } else if let Some(var_idx) = parameters.variadic() {
                        if view.details.is_some_and(|details| {
                            details
                                .extra_named
                                .iter()
                                .any(|(_, existing)| existing.as_bytes() == bytes)
                        }) {
                            return Err(NativeError::error(format!(
                                "Named parameter ${} overwrites previous argument",
                                String::from_utf8_lossy(bytes)
                            )));
                        }
                        (
                            (call.sent as usize).max(var_idx),
                            true,
                            true,
                            Some(name.to_php_string()),
                        )
                    } else {
                        return Err(NativeError::error(format!(
                            "Unknown named parameter ${}",
                            String::from_utf8_lossy(bytes)
                        )));
                    }
                }
                ArrayKey::String(name) => {
                    let bytes = name.as_bytes();
                    let view = CallView::new(&self.code, call, self.call_stack.top_details());
                    let parameters = view.parameters();
                    if let Some(pos) = parameters.position_of(bytes) {
                        let slot = slot_for(pos);
                        if self
                            .registers
                            .window(slot, 1)
                            .first()
                            .is_some_and(Option::is_some)
                        {
                            return Err(NativeError::error(format!(
                                "Named parameter ${} overwrites previous argument",
                                name.to_string_lossy()
                            )));
                        }
                        (pos, false, true, None)
                    } else if let Some(var_idx) = parameters.variadic() {
                        if view.details.is_some_and(|details| {
                            details
                                .extra_named
                                .iter()
                                .any(|(_, existing)| existing == &name)
                        }) {
                            return Err(NativeError::error(format!(
                                "Named parameter ${} overwrites previous argument",
                                name.to_string_lossy()
                            )));
                        }
                        ((call.sent as usize).max(var_idx), true, true, Some(name))
                    } else {
                        return Err(NativeError::error(format!(
                            "Unknown named parameter ${}",
                            name.to_string_lossy()
                        )));
                    }
                }
            };
            let mut value = value.clone();
            {
                let parameters =
                    CallView::new(&self.code, call, self.call_stack.top_details()).parameters();
                if let Some(declaration) = parameters
                    .receiving(index)
                    .and_then(|parameter| parameters.ty(parameter))
                    && !crate::types::accepts_as_is(&value, declaration)
                {
                    match crate::types::widen_int_to_float(&value, declaration.mask) {
                        Some(float) => value = float,
                        None => {
                            call.flags.insert(CallFlags::DEFERRED_CHECKS);
                            if declaration
                                .mask
                                .intersects(phera_bytecode::TypeMask::STRING)
                                && value.is_object()
                            {
                                call.flags.insert(CallFlags::STRING_ARGUMENT);
                            }
                        }
                    }
                }
            }
            let slot = slot_for(index);
            self.registers.grow_to(slot + 1);
            self.registers.put(slot, value);
            call.mark_sent(index, !extra);
            if was_named {
                call.flags.insert(CallFlags::NAMED);
            }
            *self.call_stack.top_call_mut()? = call;
            if let Some(extra_name) = named_extra {
                self.call_stack
                    .top_details_mut()?
                    .extra_named
                    .push((index, extra_name));
                call.flags.insert(CallFlags::DETAILS);
            }
        }
        Ok(())
    }
}
