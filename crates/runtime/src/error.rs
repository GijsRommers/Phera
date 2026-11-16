//! Why the runtime could not run a script at all.

use std::fmt;
use std::path::PathBuf;

use phera_compiler::CompileError;

/// A failure before any PHP code ran. Errors raised by running code are not
/// here: those are part of the script's output and exit status, as in PHP.
#[derive(Debug)]
pub enum RuntimeError {
    /// The file could not be read.
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    /// The file is not valid PHP, or uses something Phera does not support
    /// yet. Displays as PHP's own "Parse error" or "Fatal error" text.
    Compile(CompileError),
    /// Phera's own machinery failed.
    Internal(String),
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Read { path, error } => {
                write!(f, "Could not open {}: {error}", path.display())
            }
            RuntimeError::Compile(error) => write!(f, "{error}"),
            RuntimeError::Internal(message) => write!(f, "Phera internal error: {message}"),
        }
    }
}

impl std::error::Error for RuntimeError {}

impl From<CompileError> for RuntimeError {
    fn from(error: CompileError) -> Self {
        RuntimeError::Compile(error)
    }
}
