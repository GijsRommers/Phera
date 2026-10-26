use phera_source::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    OpenTag,
    CloseTag,

    Identifier,
    Variable,

    Integer,
    Float,
    String,

    If,
    Else,
    Function,
    Return,
    Echo,
    Class,
    New,
    True,
    False,
    Null,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Dot,

    Assign,
    Equal,
    Identical,
    NotEqual,
    NotIdentical,

    Less,
    LessEqual,
    Greater,
    GreaterEqual,

    Arrow,
    DoubleColon,
    FatArrow,
    NullCoalesce,

    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,

    Semicolon,
    Comma,
    Colon,

    Eof,
}