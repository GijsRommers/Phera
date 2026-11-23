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
        // The common callback, a PHP function whose parameters all take
        // values, has nothing to warn about.
        if let Some(call) = self.call_stack.last_call()
            && let crate::calls::prepared_call::PreparedCallee::Php(function) = call.callee
            && self.code.facts(function).takes_values_only()
        {
            return;
        }
        let Ok(view) = CallView::top(&self.code, &self.call_stack) else {
            return;
        };
        let warnings: Vec<_> = (0..view.call.sent as usize)
            .filter_map(|index| self.callback_reference_warning(view, index))
            .collect();
        for (slot, warning) in warnings {
            self.make_callback_reference(slot, warning);
        }
    }

    /// A direct callback send warns before evaluating the next argument.
    pub(crate) fn warn_callback_reference_at(&mut self, index: usize) {
        let Ok(view) = CallView::top(&self.code, &self.call_stack) else {
            return;
        };
        if let Some((slot, warning)) = self.callback_reference_warning(view, index) {
            self.make_callback_reference(slot, warning);
        }
    }

    fn callback_reference_warning(
        &self,
        view: CallView<'_>,
        index: usize,
    ) -> Option<(usize, String)> {
        let parameters = view.parameters();
        let parameter = parameters.receiving(index)?;
        let slot = view.slots().slot(index);
        let [Some(value)] = self.registers.window(slot, 1) else {
            return None;
        };
        (parameters.requires_reference(parameter) && !value.is_reference()).then(|| {
            (
                slot,
                format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    view.name(),
                    index + 1,
                    parameters.name(parameter),
                ),
            )
        })
    }

    fn make_callback_reference(&mut self, slot: usize, warning: String) {
        self.diagnostic("Warning", warning);
        let temporary = Rc::new(RefCell::new(self.registers.take(slot)));
        self.registers.put(slot, Value::Reference(temporary));
    }
}
