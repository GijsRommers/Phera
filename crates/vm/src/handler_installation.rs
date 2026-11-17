//! Installing handlers validates callbacks before changing restorable request state.

use crate::{
    Vm,
    error_handlers::{ErrorHandler, ExceptionHandler, HandlerFunction},
};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

impl Vm {
    /// Null temporarily restores the built-in handler and remains restorable itself.
    pub(crate) fn install_handler(
        &mut self,
        function: HandlerFunction,
        arguments: &[Value],
    ) -> NativeResult {
        let name = if matches!(function, HandlerFunction::SetException) {
            "set_exception_handler"
        } else {
            "set_error_handler"
        };
        let callable = arguments.first().ok_or_else(|| {
            NativeError::argument_count_error(format!(
                "{name}() expects exactly 1 argument, 0 given"
            ))
        })?;
        if !callable.is_null() {
            self.resolve_callable(callable).map_err(|error| {
                self.invalid_callback(name, 1, "callback", callable, error, true)
            })?;
        }
        if matches!(function, HandlerFunction::SetException) {
            let previous = self.error_handlers.exception.take();
            self.error_handlers.exception_history.push(previous.clone());
            self.error_handlers.exception = if callable.is_null() {
                None
            } else {
                Some(ExceptionHandler {
                    callable: callable.clone(),
                    callee: self.resolve_callable(callable)?,
                })
            };
            return Ok(previous.map(|handler| handler.callable).unwrap_or_default());
        }
        let levels = arguments
            .get(1)
            .map(|value| phera_value::to_int(value, &mut phera_value::NoDiagnostics))
            .transpose()?
            .unwrap_or(30719);
        let previous = self.error_handlers.error.take();
        let result = previous
            .as_ref()
            .map(|handler| handler.callable.clone())
            .unwrap_or_default();
        self.error_handlers.error_history.push(previous);
        self.error_handlers.error = (!callable.is_null()).then(|| ErrorHandler {
            callable: callable.clone(),
            levels,
        });
        Ok(result)
    }
}
