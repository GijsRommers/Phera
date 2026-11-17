//! Callback values cannot become caller aliases: `call_user_func($referenceCallback, 1)` warns.

use phera_value::Value;

use crate::Vm;
use crate::call_state::Callee;

impl Vm {
    /// Direct sends have writable paths; native callbacks only retain aliases explicitly supplied.
    pub(crate) fn warn_callback_references(
        &mut self,
        callee: &Callee,
        arguments: &[Option<Value>],
    ) {
        for (index, argument) in arguments.iter().enumerate() {
            let parameter = callee.parameters.get(index).or_else(|| {
                callee
                    .parameters
                    .last()
                    .filter(|parameter| parameter.variadic)
            });
            let Some(parameter) = parameter.filter(|parameter| parameter.by_reference) else {
                continue;
            };
            if argument.as_ref().is_none_or(Value::is_reference) {
                continue;
            }
            self.diagnostic(
                "Warning",
                format!(
                    "{}(): Argument #{} (${}) must be passed by reference, value given",
                    callee.name,
                    index + 1,
                    parameter.name
                ),
            );
        }
    }
}
