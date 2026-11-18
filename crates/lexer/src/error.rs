use phera_source::Span;

pub const BACKTICK_OPERATOR_ERROR: &str = "The backtick operator is not supported by Phera: deprecated since PHP 8.5, use shell_exec() instead";

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LexErrorKind {
    UnexpectedCharacter(char),
    UnterminatedString,
    UnterminatedHeredoc,
    UnterminatedComment,
    /// PHP syntax Phera rejects on purpose, such as backtick shell execution.
    Unsupported(&'static str),
}
