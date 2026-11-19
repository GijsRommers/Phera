//! `error_clear_last(); var_dump(error_get_last());` prints NULL until the next diagnostic.

use phera_extension_api::{NativeError, Unwind};
use phera_value::{ArrayKey, PhpArray, PhpString, Value};

use crate::{Vm, diagnostics::SourceLocation};

pub(crate) struct LastError {
    pub(crate) level: i64,
    pub(crate) message: PhpString,
    pub(crate) location: SourceLocation,
}

/// A bailout may cross a native callback before the default handler can write its output.
pub(crate) struct FatalDiagnostic {
    level: i64,
    message: PhpString,
    location: SourceLocation,
    trace: Option<Value>,
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
        let diagnostic = self.prepare_fatal(level, message.into(), location, None);
        self.report_prepared_fatal(diagnostic);
    }

    pub(crate) fn fatal_error(
        &mut self,
        level: i64,
        message: impl Into<PhpString>,
        location: SourceLocation,
    ) -> NativeError {
        NativeError::Unwind(Unwind(Box::new(self.prepare_fatal(
            level,
            message.into(),
            location,
            None,
        ))))
    }

    pub(crate) fn fatal_native_error(
        &mut self,
        level: i64,
        message: impl Into<PhpString>,
        name: &str,
        arguments: &[Value],
    ) -> NativeError {
        let location = self.location();
        let native = self
            .error_handlers
            .fatal_error_backtraces
            .then(|| crate::error_traces::NativeTrace::new(name, arguments));
        NativeError::Unwind(Unwind(Box::new(self.prepare_fatal(
            level,
            message.into(),
            location,
            native.as_ref(),
        ))))
    }

    fn prepare_fatal(
        &mut self,
        level: i64,
        message: PhpString,
        location: SourceLocation,
        native: Option<&crate::error_traces::NativeTrace>,
    ) -> FatalDiagnostic {
        let trace = self.fatal_trace(native, &location);
        FatalDiagnostic {
            level,
            message,
            location,
            trace,
        }
    }

    pub(crate) fn report_prepared_fatal(&mut self, diagnostic: FatalDiagnostic) {
        self.error_handlers.fatal_trace = diagnostic.trace;
        self.report_current_diagnostic(
            diagnostic.level,
            "Fatal error",
            diagnostic.message,
            diagnostic.location,
        );
        self.fiber_bailout = true;
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
