use phera_lexer::{BACKTICK_OPERATOR_ERROR, LexError, LexErrorKind};
use phera_source::Span;

pub(crate) const MULTIPLE_ACCESS_MODIFIERS: &str = "Multiple access type modifiers are not allowed";

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
            LexErrorKind::Unsupported(BACKTICK_OPERATOR_ERROR) => BACKTICK_OPERATOR_ERROR.into(),
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
    /// support on purpose, such as backticks and `(real)`.
    pub fn is_unsupported_lex(&self) -> bool {
        self.is_backtick_operator() || self.message.starts_with("unsupported syntax:")
    }

    pub fn is_backtick_operator(&self) -> bool {
        self.message == BACKTICK_OPERATOR_ERROR
    }

    /// A grammar production can reject a declaration with PHP's fatal severity.
    pub fn is_fatal(&self) -> bool {
        self.is_backtick_operator() || self.is_compile_error()
    }

    /// PHP's grammar can accept a declaration before rejecting its modifier combination.
    pub fn is_compile_error(&self) -> bool {
        self.message == MULTIPLE_ACCESS_MODIFIERS
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} at byte {}", self.message, self.span.start)
    }
}

impl std::error::Error for ParseError {}
