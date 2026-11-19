//! Publish PHP diagnostics and fatal traces at the current bytecode location.

use phera_extension_api::NativeError;
use phera_value::CollectDiagnostics;

use crate::Vm;

/// An owned source location survives mutable diagnostic and coercion work.
#[derive(Clone)]
pub(crate) struct SourceLocation {
    pub(crate) file: String,
    pub(crate) line: usize,
}

impl Vm {
    /// Snapshot the current diagnostic location before mutable VM work.
    pub(crate) fn location(&self) -> SourceLocation {
        if let Some(location) = &self.diagnostic_location {
            return location.clone();
        }
        if let Some(site) = self.attribute_call_site() {
            return SourceLocation {
                file: site.file.clone(),
                line: site.line,
            };
        }
        self.current()
            .map(|frame| SourceLocation {
                file: frame.unit().file.clone(),
                line: frame.line(),
            })
            .unwrap_or(SourceLocation {
                file: "Unknown".into(),
                line: 0,
            })
    }

    /// Flush borrowed operator diagnostics through the same owner as conversions.
    pub(crate) fn flush_operator_diagnostics(&mut self) {
        drop(OperatorDiagnostics::new(self));
    }

    /// Drain entries without discarding the reusable allocation.
    pub(crate) fn diagnostics_flush_reuse(&mut self, diagnostics: &mut CollectDiagnostics) {
        for (kind, message) in diagnostics.entries.drain(..) {
            let kind = match kind {
                phera_value::DiagnosticKind::Warning => "Warning",
                phera_value::DiagnosticKind::Notice => "Notice",
                phera_value::DiagnosticKind::Deprecated => "Deprecated",
            };
            self.diagnostic(kind, message);
        }
    }

    pub(crate) fn warn(&mut self, message: String) {
        self.diagnostic("Warning", message)
    }

    pub(crate) fn diagnostic(&mut self, kind: &str, message: String) {
        self.diagnostic_bytes(kind, message.as_bytes());
    }

    pub(crate) fn diagnostic_bytes(&mut self, kind: &str, message: &[u8]) {
        if self.queue_diagnostic(kind, message) {
            return;
        }
        let level = match kind {
            "Warning" => 2,
            "Notice" => 8,
            "Deprecated" => 8192,
            _ => 0,
        };
        if self.diagnostic_reporter.is_visible() && self.error_handlers.reporting & level != 0 {
            let SourceLocation { file, line } = self.location();
            if let Some(bytes) = self.diagnostic_reporter.report(kind, message, &file, line) {
                self.out(&bytes);
            }
        }
    }

    /// Fatal traces describe heap frames; unwinding never relies on a Rust panic.
    pub(crate) fn unwind(&mut self, error: NativeError) {
        self.diagnostic_reporter.end_all_silence();
        let error = self.materialize_error(error);
        match error {
            NativeError::Throw { class, message, .. } => {
                let SourceLocation { file, line } = self.location();
                let text = self.error_trace(class, &message, &file, line);
                self.out(text.as_bytes());
                self.status = 255;
            }
            NativeError::Unwind(state) => {
                if state.0.is::<crate::exceptions::Thrown>() {
                    if let Ok(thrown) = state.0.downcast::<crate::exceptions::Thrown>() {
                        self.report_throwable(&thrown.object);
                    }
                } else if let Ok(failure) = state
                    .0
                    .downcast::<crate::error_traces::DeclarationFailure>()
                {
                    self.out(
                        format!(
                            "\nFatal error: {} in {} on line {}\n",
                            failure.message, failure.file, failure.line
                        )
                        .as_bytes(),
                    );
                    self.status = 255;
                }
            }
        }
        self.call_stack.clear();
        self.finally_stack.clear();
    }
}

/// A conversion owns the buffer until drop, including PHP exceptions and early returns.
pub(crate) struct OperatorDiagnostics<'vm> {
    pub(crate) vm: &'vm mut Vm,
    pub(crate) buffer: CollectDiagnostics,
}

impl<'vm> OperatorDiagnostics<'vm> {
    /// Detach the reusable buffer while retaining the VM needed to publish it on every exit.
    pub(crate) fn new(vm: &'vm mut Vm) -> Self {
        let buffer = vm.diagnostic_reporter.take_operator();
        Self { vm, buffer }
    }
}

impl Drop for OperatorDiagnostics<'_> {
    fn drop(&mut self) {
        self.vm.diagnostics_flush_reuse(&mut self.buffer);
        self.vm
            .diagnostic_reporter
            .restore_operator(std::mem::take(&mut self.buffer));
    }
}

#[cfg(test)]
mod tests {
    use super::OperatorDiagnostics;
    use crate::{Vm, intrinsics::Intrinsic};
    use phera_extension_api::Registry;
    use phera_value::{PhpArray, Value, to_php_string};
    use std::{rc::Rc, sync::Arc};

    /// An intrinsic must flush an already pending operator warning instead of discarding it.
    #[tokio::test(flavor = "current_thread")]
    async fn intrinsic_keeps_pending_conversion_diagnostics() {
        let mut vm = Vm::new(Arc::new(Registry::new()));
        let value = Value::Array(Rc::new(PhpArray::new()));
        to_php_string(&value, vm.diagnostic_reporter.operator()).unwrap();
        vm.intrinsic_origin(
            Intrinsic::FunctionExists,
            vec![Value::from("unknown")],
            crate::calls::callback_binding::ArgumentOrigin::Php,
        )
        .await
        .unwrap();
        assert!(String::from_utf8_lossy(&vm.output).contains("Array to string conversion"));
        assert!(!vm.diagnostic_reporter.has_pending());
    }

    /// Returning an error while owning a buffer still publishes and drains its warnings.
    #[test]
    fn conversion_error_drops_and_flushes_owner() {
        let mut vm = Vm::new(Arc::new(Registry::new()));
        let convert = |vm: &mut Vm| -> Result<(), phera_extension_api::NativeError> {
            let mut diagnostics = OperatorDiagnostics::new(vm);
            to_php_string(
                &Value::Array(Rc::new(PhpArray::new())),
                &mut diagnostics.buffer,
            )?;
            Err(phera_extension_api::NativeError::error("conversion failed"))
        };
        assert!(convert(&mut vm).is_err());
        assert!(String::from_utf8_lossy(&vm.output).contains("Array to string conversion"));
        assert!(!vm.diagnostic_reporter.has_pending());
    }
}
