//! Native callbacks retain only explicitly supplied reference cells; other values warn.

use crate::Vm;

impl Vm {
    /// Direct sends have writable paths; native callers supply only already evaluated values.
    pub(crate) fn warn_callback_references(&mut self) {
        let count = self
            .call_stack
            .preparing
            .last()
            .map_or(0, |call| call.window.supplied);
        for index in 0..count {
            let warning = self.call_stack.preparing.last().and_then(|call| {
                let parameter = self
                    .signatures
                    .get(call.callee.signature)
                    .parameters
                    .get(index)
                    .or_else(|| {
                        self.signatures
                            .get(call.callee.signature)
                            .parameters
                            .last()
                            .filter(|parameter| parameter.variadic)
                    })?;
                let value = self
                    .registers
                    .window(call.window.slot(index), 1)
                    .first()?
                    .as_ref()?;
                if !parameter.by_reference || value.is_reference() {
                    return None;
                }
                Some(format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    self.signatures.get(call.callee.signature).name,
                    index + 1,
                    parameter.name,
                ))
            });
            if let Some(warning) = warning {
                self.diagnostic("Warning", warning);
            }
        }
    }
}
