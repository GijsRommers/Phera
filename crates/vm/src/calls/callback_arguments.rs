//! Array callbacks route keys through the same preparation stack as unpacking.

use phera_extension_api::{LocalFuture, NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray};

use crate::Vm;
use crate::calls::call_state::{ArgumentOrder, Callee};
use crate::calls::callback_binding::ArgumentOrigin;

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
        let depth = self.call_stack.preparing.len();
        let routed = self
            .prepare_call(callee)
            .and_then(|()| self.route_callback_array(array));
        self.finish_callback(routed, base, depth, origin)
    }

    /// Integer keys follow positional order and string keys select declared or variadic slots.
    fn route_callback_array(&mut self, array: &PhpArray) -> Result<(), NativeError> {
        for (key, value) in array.iter() {
            let call = self.call_stack.pending_mut()?;
            match key {
                ArrayKey::Int(_) if call.window.order == ArgumentOrder::Named => {
                    return Err(NativeError::error(
                        "Cannot use positional argument after named argument",
                    ));
                }
                ArrayKey::Int(_) => {}
                ArrayKey::String(name) => call.window.named = Some(name.clone()),
            }
            let index = call.argument_index()?;
            self.store_sent_value(index, value.clone())?;
        }
        Ok(())
    }
}
