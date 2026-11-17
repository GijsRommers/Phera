//! Ordinary value sends do not need named routing or mutable PHP places.

use crate::Vm;
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

impl Vm {
    /// Mirrors send for positional value arguments whose parameter is by value.
    /// Named and reference parameters retain the full routing and error path.
    pub(crate) fn send_value(&mut self, source: Reg) -> Result<(), NativeError> {
        if self.send_positional_value(source) {
            return Ok(());
        }
        self.send(Some(source), None)
    }

    /// Borrow signature and registers directly instead of cloning the callee to
    /// cross mutable place resolution, which this guarded path never performs.
    fn send_positional_value(&mut self, source: Reg) -> bool {
        let Some(frame) = self.call_stack.current_mut() else {
            return false;
        };
        let Some(call) = frame.calls.last_mut() else {
            return false;
        };
        let parameters = &call.callee.parameters;
        let parameter = parameters
            .get(call.arguments.len())
            .or_else(|| parameters.last().filter(|parameter| parameter.variadic));
        if call.named.is_some()
            || call.extra_named.is_some()
            || parameter.is_some_and(|parameter| parameter.by_reference)
        {
            return false;
        }
        let value = self
            .registers
            .window(frame.base, frame.count)
            .get(source.index())
            .and_then(Option::as_ref)
            .map(|value| match value {
                Value::Int(integer) => Value::Int(*integer),
                value => value.deref().clone(),
            })
            .unwrap_or_default();
        call.arguments.push(Some(value));
        true
    }
}
