use phera_lexer::{LexError, LexErrorKind};
use phera_source::Span;

/// The first thing in the source that did not fit the grammar.
///
/// A lexer error is reported as a `ParseError` with the lexer's span.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    pub(crate) fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub(crate) fn from_lex(error: &LexError) -> Self {
        let message = match &error.kind {
            LexErrorKind::Unsupported(feature) => format!("unsupported syntax: {feature}"),
            LexErrorKind::UnexpectedCharacter(c) => format!("unexpected character {c:?}"),
            LexErrorKind::UnterminatedString => "unterminated string".to_string(),
            LexErrorKind::UnterminatedHeredoc => "unterminated heredoc".to_string(),
            LexErrorKind::UnterminatedComment => "unterminated comment".to_string(),
        };
        Self {
            message,
            span: error.span,
        }
    }

    /// Whether this error came from the lexer rejecting syntax Phera does not
    /// support on purpose (backticks, `${...}` interpolation, `(real)`, ...).
    pub fn is_unsupported_lex(&self) -> bool {
        self.message.starts_with("unsupported syntax:")
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.span.start)
    }
}

impl std::error::Error for ParseError {}
