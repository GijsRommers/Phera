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
    UnterminatedComment,
    /// Valid PHP the lexer doesn't handle yet, such as string interpolation.
    Unsupported(&'static str),
}
