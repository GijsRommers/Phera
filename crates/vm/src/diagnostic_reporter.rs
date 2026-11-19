//! `@trigger_error('hidden')` temporarily replaces the active reporting mask.

use phera_value::CollectDiagnostics;

use crate::Vm;

const FATAL_REPORTING_MASK: i64 = 4437;

/// Owns the request state for diagnostic reporter.
pub(crate) struct DiagnosticReporter {
    /// Request-owned operator diagnostics.
    operator_diagnostics: CollectDiagnostics,
    /// Request-owned suppression.
    suppression: Vec<i64>,
    /// Request-owned diagnostic capture.
    diagnostic_capture: Option<Vec<Vec<u8>>>,
}

impl DiagnosticReporter {
    pub(crate) fn new() -> Self {
        Self {
            operator_diagnostics: CollectDiagnostics::default(),
            suppression: Vec::new(),
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

    pub(crate) fn silence(&self) -> usize {
        self.suppression.len()
    }

    pub(crate) fn suspend_silence(&mut self, depth: usize) -> Vec<i64> {
        if depth < self.suppression.len() {
            self.suppression.split_off(depth)
        } else {
            Vec::new()
        }
    }

    pub(crate) fn resume_silence(&mut self, masks: Vec<i64>) {
        self.suppression.extend(masks);
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

impl Vm {
    pub(crate) fn begin_silence(&mut self) {
        let reporting = self.error_handlers.reporting;
        self.diagnostic_reporter.suppression.push(reporting);
        if reporting & !FATAL_REPORTING_MASK != 0 {
            self.error_handlers.reporting &= FATAL_REPORTING_MASK;
            self.mark_reporting_ini_modified();
        }
    }

    pub(crate) fn end_silence(&mut self) {
        if let Some(saved) = self.diagnostic_reporter.suppression.pop()
            && self.error_handlers.reporting & !FATAL_REPORTING_MASK == 0
            && saved & !FATAL_REPORTING_MASK != 0
        {
            self.error_handlers.reporting = saved;
        }
    }

    /// Unwinding an abandoned expression has the same mask rules as leaving it normally.
    pub(crate) fn restore_silence(&mut self, depth: usize) {
        while self.diagnostic_reporter.silence() > depth {
            self.end_silence();
        }
    }
}
