//! Request-local diagnostic reporter state kept separate from register execution.

use phera_value::CollectDiagnostics;

/// Owns the request state for diagnostic reporter.
pub(crate) struct DiagnosticReporter {
    /// Request-owned operator diagnostics.
    operator_diagnostics: CollectDiagnostics,
    /// Request-owned suppression.
    suppression: usize,
    /// Request-owned diagnostic capture.
    diagnostic_capture: Option<Vec<Vec<u8>>>,
}

impl DiagnosticReporter {
    pub(crate) fn new() -> Self {
        Self {
            operator_diagnostics: CollectDiagnostics::default(),
            suppression: 0,
            diagnostic_capture: None,
        }
    }
}

impl DiagnosticReporter {
    /// Operators borrow a reusable collector without retaining a mutable VM borrow.
    pub(crate) fn take_operator(&mut self) -> CollectDiagnostics {
        std::mem::take(&mut self.operator_diagnostics)
    }

    pub(crate) fn restore_operator(&mut self, diagnostics: CollectDiagnostics) {
        self.operator_diagnostics = diagnostics;
    }

    pub(crate) fn operator(&mut self) -> &mut CollectDiagnostics {
        &mut self.operator_diagnostics
    }

    pub(crate) fn has_pending(&self) -> bool {
        !self.operator_diagnostics.entries.is_empty()
    }

    /// PHP @ suppresses ordinary diagnostics within its nested expression.
    pub(crate) fn silence(&self) -> usize {
        self.suppression
    }

    pub(crate) fn begin_silence(&mut self) {
        self.suppression += 1;
    }

    pub(crate) fn end_silence(&mut self) {
        self.suppression = self.suppression.saturating_sub(1);
    }

    /// Fatal errors remain visible even when raised inside @.
    /// Cold unwind paths restore suppression from lexical instruction ranges.
    pub(crate) fn restore_silence(&mut self, depth: usize) {
        self.suppression = depth;
    }

    pub(crate) fn end_all_silence(&mut self) {
        self.suppression = 0;
    }

    pub(crate) fn is_visible(&self) -> bool {
        self.suppression == 0
    }

    /// Captured diagnostics do not reach the script's output.
    pub(super) fn display(&mut self, bytes: Vec<u8>) -> Option<Vec<u8>> {
        if let Some(capture) = &mut self.diagnostic_capture {
            capture.push(bytes);
            None
        } else {
            Some(bytes)
        }
    }
}
