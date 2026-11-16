//! Compiler diagnostics keep PHP location and parse-versus-fatal presentation.

/// A source diagnostic suitable for PHP CLI rendering.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
    pub file: String,
    pub line: usize,
    pub parse: bool,
}
impl std::fmt::Display for CompileError {
    fn fmt(&self, function: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
