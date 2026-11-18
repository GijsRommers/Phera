//! A callback called by native code gets values, not variables. A parameter
//! declared by reference then warns and works on a temporary:
//!
//! ```php
//! call_user_func('array_pop', [1, 2]);
//! // Warning: array_pop(): Argument #1 ($array) must be passed by reference, value given
//! // and the result is 2: array_pop ran on a copy that is then dropped
//! ```
//!
//! Reference cells the native caller supplied on purpose (`array_walk`,
//! `preg_match`'s `$matches`) are kept as they are.

use std::cell::RefCell;
use std::rc::Rc;

use phera_value::Value;

use crate::Vm;

impl Vm {
    /// Warn about each value given to a by-reference parameter, and put it
    /// in a reference cell of its own, so that the callee finds a reference
    /// there as it does on every other path.
    pub(crate) fn warn_callback_references(&mut self) {
        let count = self
            .call_stack
            .preparing
            .last()
            .map_or(0, |call| call.window.supplied);
        for index in 0..count {
            let warning = self.call_stack.preparing.last().and_then(|call| {
                let parameter = call.callee.parameters.get(index).or_else(|| {
                    call.callee
                        .parameters
                        .last()
                        .filter(|parameter| parameter.variadic)
                })?;
                let slot = call.window.slot(index);
                let value = self.registers.window(slot, 1).first()?.as_ref()?;
                if !parameter.by_reference || value.is_reference() {
                    return None;
                }
                let warning = format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    call.callee.name,
                    index + 1,
                    parameter.name,
                );
                Some((slot, warning))
            });
            if let Some((slot, warning)) = warning {
                self.diagnostic("Warning", warning);
                let temporary = Rc::new(RefCell::new(self.registers.take(slot)));
                self.registers.put(slot, Value::Reference(temporary));
            }
        }
    }
}
