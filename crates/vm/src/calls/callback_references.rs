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
use crate::calls::call_view::CallView;

impl Vm {
    /// Warn about each value given to a by-reference parameter, and put it
    /// in a reference cell of its own, so that the callee finds a reference
    /// there as it does on every other path.
    pub(crate) fn warn_callback_references(&mut self) {
        let Ok(view) = CallView::top(&self.code, &self.call_stack) else {
            return;
        };
        let (parameters, slots) = (view.parameters(), view.slots());
        let mut warnings = Vec::new();
        for index in 0..view.call.sent as usize {
            let Some(parameter) = parameters.receiving(index) else {
                continue;
            };
            let slot = slots.slot(index);
            let Some(Some(value)) = self.registers.window(slot, 1).first() else {
                continue;
            };
            if parameters.by_reference(parameter) && !value.is_reference() {
                let warning = format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    view.name(),
                    index + 1,
                    parameters.name(parameter),
                );
                warnings.push((slot, warning));
            }
        }
        for (slot, warning) in warnings {
            self.diagnostic("Warning", warning);
            let temporary = Rc::new(RefCell::new(self.registers.take(slot)));
            self.registers.put(slot, Value::Reference(temporary));
        }
    }
}
