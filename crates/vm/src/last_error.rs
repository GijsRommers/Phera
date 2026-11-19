//! `error_clear_last(); var_dump(error_get_last());` prints NULL until the next diagnostic.

use phera_value::{ArrayKey, PhpArray, PhpString, Value};

use crate::{Vm, diagnostics::SourceLocation};

pub(crate) struct LastError {
    pub(crate) level: i64,
    pub(crate) message: PhpString,
    pub(crate) location: SourceLocation,
}

impl LastError {
    pub(crate) fn to_value(&self, fatal_trace: Option<&Value>) -> Value {
        let mut error = PhpArray::new();
        error.insert(ArrayKey::str("type"), Value::Int(self.level));
        error.insert(
            ArrayKey::str("message"),
            Value::String(self.message.clone()),
        );
        error.insert(
            ArrayKey::str("file"),
            Value::from(self.location.file.as_str()),
        );
        error.insert(ArrayKey::str("line"), Value::Int(self.location.line as i64));
        if let Some(trace) = fatal_trace {
            error.insert(ArrayKey::str("trace"), trace.clone());
        }
        Value::from(error)
    }
}

impl Vm {
    pub(crate) fn report_fatal(
        &mut self,
        level: i64,
        message: impl Into<PhpString>,
        location: SourceLocation,
    ) {
        self.capture_fatal_trace(None, &location);
        self.report_current_diagnostic(level, "Fatal error", message.into(), location);
        self.status = 255;
    }

    /// A diagnostic reaches this path only when PHP's default handler owns it.
    pub(crate) fn report_diagnostic(
        &mut self,
        level: i64,
        kind: &str,
        message: PhpString,
        location: SourceLocation,
    ) {
        self.error_handlers.fatal_trace = None;
        self.report_current_diagnostic(level, kind, message, location);
    }

    pub(crate) fn report_current_diagnostic(
        &mut self,
        level: i64,
        kind: &str,
        message: PhpString,
        location: SourceLocation,
    ) {
        if self.error_handlers.reporting & level != 0 {
            let trace = self.error_handlers.fatal_trace.as_ref().map(|trace| {
                crate::exception_rendering::render_trace(
                    trace,
                    self.error_handlers.trace_string_limit,
                )
            });
            if self.error_handlers.log_errors && self.diagnostic_reporter.is_visible() {
                use std::io::Write;

                let mut log = std::io::stderr().lock();
                let _ = write!(log, "PHP {kind}:  ");
                let _ = log.write_all(message.as_bytes());
                let _ = write!(log, " in {} on line {}", location.file, location.line);
                if let Some(trace) = &trace {
                    let _ = write!(log, "\nStack trace:\n{trace}");
                }
                let _ = writeln!(log);
            }
            if self.error_handlers.display_errors
                && let Some(bytes) = self.diagnostic_reporter.report(
                    kind,
                    message.as_bytes(),
                    &location.file,
                    location.line,
                    trace.as_deref(),
                )
            {
                self.out(&bytes);
            }
        }
        self.record_last_error(level, message, location);
    }

    pub(crate) fn record_last_error(
        &mut self,
        level: i64,
        message: impl Into<PhpString>,
        location: SourceLocation,
    ) {
        self.error_handlers.last_error = Some(LastError {
            level,
            message: message.into(),
            location,
        });
    }
}
