//! Request-level error and exception handlers share the ordinary callable resolver.
//!
//! Diagnostics has a synchronous, infallible extension API. When a PHP handler
//! is installed, warnings are queued until the current instruction completes;
//! the interpreter then calls PHP before fetching another instruction. Recursive
//! warnings use built-in reporting while the handler is active.

use crate::{Vm, diagnostics::SourceLocation};
use phera_extension_api::{Context, NativeError, NativeResult};
use phera_value::Value;

/// Callable plus the error-level mask accepted by set_error_handler.
#[derive(Clone)]
pub(crate) struct ErrorHandler {
    pub(crate) callable: Value,
    pub(crate) levels: i64,
}

/// Retain the source before calling a handler changes the active frame.
pub(crate) struct DiagnosticEvent {
    pub(crate) level: i64,
    pub(crate) kind: String,
    pub(crate) message: String,
    pub(crate) location: SourceLocation,
    pub(crate) native: Option<crate::error_traces::NativeTrace>,
}

/// Resolve at registration so private scope and method callables survive unwinding.
#[derive(Clone)]
pub(crate) struct ExceptionHandler {
    pub(crate) callable: Value,
    pub(crate) callee: crate::calls::call_state::Callee,
}

/// All handler stacks and masks are request-local and retain PHP callable values.
pub(crate) struct ErrorHandlers {
    pub(crate) exception: Option<ExceptionHandler>,
    pub(crate) exception_history: Vec<Option<ExceptionHandler>>,
    pub(crate) error: Option<ErrorHandler>,
    pub(crate) error_history: Vec<Option<ErrorHandler>>,
    pub(crate) active: bool,
    /// Native warning sites are retained only while PHP error handlers are installed.
    pub(crate) native_site: Option<crate::error_traces::NativeTrace>,
    pub(crate) warning_site: Option<crate::exception_traces::WarningSite>,
    pub(crate) pending: Vec<DiagnosticEvent>,
    pub(crate) reporting: i64,
    /// PHP formats saved trace strings using the current request limit, not creation-time text.
    pub(crate) trace_string_limit: usize,
}

impl Default for ErrorHandlers {
    fn default() -> Self {
        Self {
            exception: None,
            exception_history: Vec::new(),
            error: None,
            error_history: Vec::new(),
            active: false,
            native_site: None,
            warning_site: None,
            pending: Vec::new(),
            reporting: 30719,
            trace_string_limit: 15,
        }
    }
}

/// Handler intrinsics need asynchronous access to PHP callables.
#[derive(Clone, Copy)]
pub(crate) enum HandlerFunction {
    SetException,
    RestoreException,
    SetError,
    RestoreError,
    TriggerError,
}

impl Vm {
    /// Installing or restoring handlers returns PHP's previous callable or success flag.
    pub(crate) async fn handler_intrinsic(
        &mut self,
        function: HandlerFunction,
        mut arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        let signature = function.signature();
        let tracking = self.error_handlers.error.is_some() && !self.error_handlers.active;
        let previous = if tracking {
            self.error_handlers
                .native_site
                .replace(crate::error_traces::NativeTrace::new(
                    signature.name,
                    &arguments,
                ))
        } else {
            None
        };
        let prepared = self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(
                signature.name,
                signature.parameters,
                &mut arguments,
                vm,
            )
        });
        if tracking {
            self.error_handlers.native_site = previous;
        }
        if let Err(error) = prepared {
            self.failed_native = Some(crate::error_traces::NativeTrace::new(
                signature.name,
                &arguments,
            ));
            return Err(error);
        }
        if !self.error_handlers.pending.is_empty() {
            self.drain_error_handlers().await?;
        }
        match function {
            HandlerFunction::SetException | HandlerFunction::SetError => {
                if let Some(callback) = arguments.first() {
                    self.autoload_callable(callback).await?;
                }
                self.install_handler(function, &arguments)
            }
            HandlerFunction::RestoreException => {
                self.error_handlers.exception =
                    self.error_handlers.exception_history.pop().flatten();
                Ok(Value::Bool(true))
            }
            HandlerFunction::RestoreError => {
                self.error_handlers.error = self.error_handlers.error_history.pop().flatten();
                Ok(Value::Bool(true))
            }
            HandlerFunction::TriggerError => self.trigger_error(&arguments).await,
        }
    }

    /// Return false only from the user handler requests PHP's built-in diagnostic.
    pub(crate) async fn drain_error_handlers(&mut self) -> Result<(), NativeError> {
        let pending = std::mem::take(&mut self.error_handlers.pending);
        for event in pending {
            let handler = self.error_handlers.error.clone();
            let handled = if let Some(handler) =
                handler.filter(|handler| handler.levels & event.level != 0)
            {
                self.error_handlers.active = true;
                let warning_site =
                    event
                        .native
                        .as_ref()
                        .map(|native| crate::exception_traces::WarningSite {
                            boundary: self.call_stack.depth(),
                            native: native.clone(),
                            location: event.location.clone(),
                        });
                let previous_site =
                    std::mem::replace(&mut self.error_handlers.warning_site, warning_site);
                let result = self
                    .call(
                        &handler.callable,
                        vec![
                            Value::Int(event.level),
                            Value::from(event.message.clone()),
                            Value::from(event.location.file.clone()),
                            Value::Int(event.location.line as i64),
                        ],
                    )
                    .await;
                self.error_handlers.active = false;
                self.error_handlers.warning_site = previous_site;
                !matches!(result?, Value::Bool(false))
            } else {
                false
            };
            if !handled {
                self.report_event(&event);
                if event.level == 256 {
                    self.status = 255;
                    return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                        (),
                    ))));
                }
            }
        }
        Ok(())
    }

    /// Queuing is required even under @ or error_reporting(0): PHP still calls handlers.
    pub(crate) fn queue_diagnostic(&mut self, kind: &str, message: &str) -> bool {
        let level = match kind {
            "Warning" => 2,
            "Notice" => 8,
            "Deprecated" => 8192,
            _ => 0,
        };
        if !self.error_handlers.active
            && self
                .error_handlers
                .error
                .as_ref()
                .is_some_and(|handler| handler.levels & level != 0)
        {
            self.error_handlers.pending.push(DiagnosticEvent {
                level,
                kind: kind.into(),
                message: message.into(),
                location: self.location(),
                native: self.error_handlers.native_site.clone(),
            });
            true
        } else {
            false
        }
    }

    /// Respect the reporting mask separately from whether a user callback was invoked.
    pub(crate) fn report_event(&mut self, event: &DiagnosticEvent) {
        if self.error_handlers.reporting & event.level != 0
            && let Some(bytes) = self.diagnostic_reporter.report(
                &event.kind,
                event.message.clone(),
                &event.location.file,
                event.location.line,
            )
        {
            self.out(&bytes);
        }
    }
}

impl Vm {
    /// The last-resort callback runs once after the PHP stack has unwound completely.
    pub(crate) async fn invoke_exception_handler(&mut self, error: &NativeError) -> bool {
        let NativeError::Unwind(payload) = error else {
            return false;
        };
        let Some(thrown) = payload.0.downcast_ref::<crate::exceptions::Thrown>() else {
            return false;
        };
        let Some(callable) = self.error_handlers.exception.take() else {
            return false;
        };
        let object = Value::Object(thrown.object.clone());
        if let Err(error) = self
            .invoke_resolved_callable(callable.callee, vec![object])
            .await
        {
            self.unwind(error);
        }
        true
    }
}

impl Vm {
    /// Zend leaves a returning frame's catch region before invoking its warning callback.
    pub(crate) async fn drain_return_handlers(&mut self) -> Result<(), NativeError> {
        let depth = self.call_stack.depth();
        self.drain_error_handlers()
            .await
            .map_err(|error| match error {
                NativeError::Unwind(mut payload) => {
                    if let Some(thrown) = payload.0.downcast_mut::<crate::exceptions::Thrown>() {
                        thrown.skip_handlers = Some(depth);
                    }
                    NativeError::Unwind(payload)
                }
                error => error,
            })
    }
}
