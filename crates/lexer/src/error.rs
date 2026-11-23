use phera_source::Span;

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
    /// PHP syntax Phera rejects on purpose, such as the removed (real) cast.
    Unsupported(&'static str),
}
