//! Scanner diagnostics as emitted for PHP source such as `echo "\400";`.

/// Zend distinguishes warnings from the scanner from ordinary runtime warnings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompileDiagnosticKind {
    Warning,
    Deprecated,
}

impl CompileDiagnosticKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Warning => "Warning",
            Self::Deprecated => "Deprecated",
        }
    }

    pub fn error_level(self) -> i64 {
        match self {
            Self::Warning => 128,     // E_COMPILE_WARNING
            Self::Deprecated => 8192, // E_DEPRECATED
        }
    }
}

/// Source location and text of a nonfatal scanner diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileDiagnostic {
    pub kind: CompileDiagnosticKind,
    pub message: String,
    pub line: usize,
}

impl CompileDiagnostic {
    pub fn render(&self, file: &str, line: usize) -> String {
        format!(
            "\n{}: {} in {file} on line {line}\n",
            self.kind.label(),
            self.message,
        )
    }
}

/// Where PHP reports a diagnostic from a native scanner.
pub enum CompileDiagnosticSite<'a> {
    /// Raw tokenization runs without a compiled source location.
    Caller,
    /// Parsed tokenizer strings use the empty filename. Files use their path.
    Source(&'a str),
}
