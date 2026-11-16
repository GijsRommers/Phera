//! Publish PHP diagnostics and fatal traces at the current bytecode location.

use phera_extension_api::NativeError;
use phera_value::CollectDiagnostics;

use crate::Vm;

/// An owned source location survives mutable diagnostic and coercion work.
pub(crate) struct SourceLocation {
    pub(crate) file: String,
    pub(crate) line: usize,
}

impl Vm {
    /// Snapshot the current diagnostic location before mutable VM work.
    pub(crate) fn location(&self) -> SourceLocation {
        self.current()
            .map(|frame| SourceLocation {
                file: frame.unit.file.clone(),
                line: frame.line(),
            })
            .unwrap_or(SourceLocation {
                file: "Unknown".into(),
                line: 0,
            })
    }

    /// Publish diagnostics while retaining their allocation for the next operator.
    pub(crate) fn diagnostics_flush(&mut self, mut diagnostics: CollectDiagnostics) {
        self.diagnostics_flush_reuse(&mut diagnostics);
        self.diagnostic_reporter.restore_operator(diagnostics);
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
        if self.diagnostic_reporter.is_visible() {
            let SourceLocation { file, line } = self.location();
            if let Some(bytes) = self.diagnostic_reporter.report(kind, message, &file, line) {
                self.out(&bytes);
            }
        }
    }

    /// Fatal traces describe heap frames; unwinding never relies on a Rust panic.
    pub(crate) fn unwind(&mut self, error: NativeError) {
        self.diagnostic_reporter.end_all_silence();
        if let NativeError::Throw { class, message } = error {
            let SourceLocation { file, line } = self.location();
            let text = self.error_trace(class, &message, &file, line);
            self.out(text.as_bytes());
            self.status = 255;
        }
        self.call_stack.clear();
    }
}
