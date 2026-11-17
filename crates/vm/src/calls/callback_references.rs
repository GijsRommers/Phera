//! Native callbacks retain only explicitly supplied reference cells; other values warn.

use crate::Vm;
use crate::calls::call_view::CallView;

impl Vm {
    /// Direct sends have writable paths; native callers supply only already evaluated values.
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
            let value = self.registers.window(slots.slot(index), 1).first();
            let Some(Some(value)) = value else {
                continue;
            };
            if parameters.by_reference(parameter) && !value.is_reference() {
                warnings.push(format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    view.name(),
                    index + 1,
                    parameters.name(parameter),
                ));
            }
        }
        for warning in warnings {
            self.diagnostic("Warning", warning);
        }
    }
}
