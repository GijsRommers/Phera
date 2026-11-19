//! `error_clear_last(); var_dump(error_get_last());` prints NULL until the next diagnostic.

use phera_value::{ArrayKey, PhpArray, PhpString, Value};

use crate::{Vm, diagnostics::SourceLocation};

pub(crate) struct LastError {
    pub(crate) level: i64,
    pub(crate) message: PhpString,
    pub(crate) location: SourceLocation,
}

impl LastError {
    pub(crate) fn to_value(&self) -> Value {
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
        Value::from(error)
    }
}

impl Vm {
    /// A diagnostic reaches this path only when PHP's default handler owns it.
    pub(crate) fn report_diagnostic(
        &mut self,
        level: i64,
        kind: &str,
        message: PhpString,
        location: SourceLocation,
    ) {
        if self.error_handlers.reporting & level != 0
            && let Some(bytes) = self.diagnostic_reporter.report(
                kind,
                message.as_bytes(),
                &location.file,
                location.line,
            )
        {
            self.out(&bytes);
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
