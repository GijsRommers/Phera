//! Request-local diagnostic reporter state kept separate from register execution.

use phera_value::CollectDiagnostics;

/// Owns the request state for diagnostic reporter.
pub(crate) struct DiagnosticReporter {
    /// Request-owned operator diagnostics.
    pub(crate) operator_diagnostics: CollectDiagnostics,
    /// Request-owned suppression.
    pub(crate) suppression: usize,
    /// Request-owned diagnostic capture.
    pub(crate) diagnostic_capture: Option<Vec<Vec<u8>>>,
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
