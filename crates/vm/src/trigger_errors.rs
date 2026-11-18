//! User diagnostics share error-handler masks, recursion guards and PHP fatal behavior.

use crate::{Vm, error_handlers::DiagnosticEvent};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

impl Vm {
    /// PHP 8.5 deprecates E_USER_ERROR before emitting its fatal user diagnostic.
    pub(crate) async fn trigger_error(&mut self, arguments: &[Value]) -> NativeResult {
        let message = arguments.first().ok_or_else(|| {
            NativeError::argument_count_error(
                "trigger_error() expects at least 1 argument, 0 given",
            )
        })?;
        let message =
            phera_value::to_php_string(message, &mut phera_value::NoDiagnostics)?.to_string_lossy();
        let level = arguments
            .get(1)
            .map(|value| phera_value::to_int(value, &mut phera_value::NoDiagnostics))
            .transpose()?
            .unwrap_or(1024);
        let kind = match level {
            256 => "Fatal error",
            512 => "Warning",
            1024 => "Notice",
            16384 => "Deprecated",
            _ => {
                return Err(NativeError::value_error(
                    "trigger_error(): Argument #2 ($error_level) must be one of E_USER_ERROR, E_USER_WARNING, E_USER_NOTICE, or E_USER_DEPRECATED",
                ));
            }
        };
        if level == 256 {
            self.diagnostic("Deprecated", "Passing E_USER_ERROR to trigger_error() is deprecated since 8.4, throw an exception or call exit with a string message instead".into());
            self.drain_error_handlers().await?;
        }
        let event = DiagnosticEvent {
            level,
            kind: kind.into(),
            message,
            location: self.location(),
            trace_location: None,
            native: Some(crate::error_traces::NativeTrace::new(
                "trigger_error",
                arguments,
            )),
        };
        let use_handler = !self.error_handlers.active
            && self
                .error_handlers
                .error
                .as_ref()
                .is_some_and(|handler| handler.levels & level != 0);
        if use_handler {
            self.error_handlers.pending.push(event);
            self.drain_error_handlers().await?;
        } else {
            self.report_event(&event);
            if level == 256 {
                self.status = 255;
                return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                    (),
                ))));
            }
        }
        Ok(Value::Bool(true))
    }
}
