use std::fmt;

/// Why a native function call failed; the runtime turns this into a PHP error.
#[derive(Debug, Clone, PartialEq)]
pub enum CallError {
    ArgumentCount {
        min: usize,
        max: usize,
        given: usize,
    },
    ArgumentType {
        position: usize,
        expected: &'static str,
        given: &'static str,
    },
    /// Raised as a PHP `Error` with this message.
    Error(String),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::ArgumentCount { min, max, given } => {
                let (bound, count) = match (given < min, min == max) {
                    (_, true) => ("exactly", min),
                    (true, false) => ("at least", min),
                    (false, false) => ("at most", max),
                };
                let noun = if *count == 1 { "argument" } else { "arguments" };
                write!(f, "expects {bound} {count} {noun}, {given} given")
            }
            CallError::ArgumentType {
                position,
                expected,
                given,
            } => write!(
                f,
                "Argument #{position} must be of type {expected}, {given} given"
            ),
            CallError::Error(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for CallError {}
