//! Errors and diagnostics for value-level operations.

use std::fmt;

/// The PHP exception/Error class an operation failure maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// Plain `Exception`/`Error` (e.g. "Cannot add element ...").
    Error,
    TypeError,
    ValueError,
    ArithmeticError,
    DivisionByZeroError,
    Native(&'static str),
}

impl ErrorKind {
    /// The name of the PHP class to throw.
    pub fn class_name(self) -> &'static str {
        match self {
            ErrorKind::Error => "Error",
            ErrorKind::TypeError => "TypeError",
            ErrorKind::ValueError => "ValueError",
            ErrorKind::ArithmeticError => "ArithmeticError",
            ErrorKind::DivisionByZeroError => "DivisionByZeroError",
            ErrorKind::Native(class) => class,
        }
    }
}

/// A failed value operation, matching what PHP throws for the same input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpError {
    pub kind: ErrorKind,
    pub message: String,
}

impl OpError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        OpError {
            kind,
            message: message.into(),
        }
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        OpError::new(ErrorKind::TypeError, message)
    }

    pub fn value_error(message: impl Into<String>) -> Self {
        OpError::new(ErrorKind::ValueError, message)
    }

    pub fn error(message: impl Into<String>) -> Self {
        OpError::new(ErrorKind::Error, message)
    }
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for OpError {}

/// Receives PHP-level warnings, notices and deprecations. Operations that would
/// emit these in real PHP take a `&mut dyn Diagnostics`.
pub trait Diagnostics {
    /// PHP float-to-string conversion uses the request's precision.
    fn float_precision(&self) -> i32 {
        14
    }

    fn warning(&mut self, message: String);
    fn notice(&mut self, message: String);
    fn deprecated(&mut self, message: String);
}

/// Ignores every diagnostic. Used where a caller does not care.
pub struct NoDiagnostics;

impl Diagnostics for NoDiagnostics {
    fn warning(&mut self, _message: String) {}
    fn notice(&mut self, _message: String) {}
    fn deprecated(&mut self, _message: String) {}
}

/// Collects diagnostics with their kind, for tests and callers that want to
/// report them later.
#[derive(Debug, Default, Clone)]
pub struct CollectDiagnostics {
    pub entries: Vec<(DiagnosticKind, String)>,
    pub float_precision: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticKind {
    Warning,
    Notice,
    Deprecated,
}

impl Diagnostics for CollectDiagnostics {
    fn float_precision(&self) -> i32 {
        self.float_precision.unwrap_or(14)
    }

    fn warning(&mut self, message: String) {
        self.entries.push((DiagnosticKind::Warning, message));
    }
    fn notice(&mut self, message: String) {
        self.entries.push((DiagnosticKind::Notice, message));
    }
    fn deprecated(&mut self, message: String) {
        self.entries.push((DiagnosticKind::Deprecated, message));
    }
}

impl CollectDiagnostics {
    pub fn messages(&self) -> Vec<&str> {
        self.entries.iter().map(|(_, m)| m.as_str()).collect()
    }
}
