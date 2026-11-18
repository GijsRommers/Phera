//! Compiler diagnostics keep PHP location and parse-versus-fatal presentation.

/// A source diagnostic suitable for PHP CLI rendering.
#[derive(Debug, Clone)]
pub struct CompileError {
    /// PHP-facing diagnostic text, without the location suffix.
    pub message: String,
    /// Source file spelling supplied by the caller.
    pub file: String,
    /// One-based source line.
    pub line: usize,
    /// Selects Parse error rather than Fatal error in Display.
    pub parse: bool,
    /// Scanner diagnostics PHP emitted before reaching the fatal error.
    pub scanner_diagnostics: Vec<phera_source::CompileDiagnostic>,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, function: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for diagnostic in &self.scanner_diagnostics {
            function.write_str(&diagnostic.render(&self.file, diagnostic.line))?;
        }
        write!(
            function,
            "\n{}: {} in {} on line {}\n",
            if self.parse {
                "Parse error"
            } else {
                "Fatal error"
            },
            self.message,
            self.file,
            self.line
        )
    }
}

impl std::error::Error for CompileError {}
