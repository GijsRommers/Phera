use phera_source::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    /// Text outside `<?php ... ?>`.
    InlineHtml,
    OpenTag,
    /// `<?=`
    OpenTagWithEcho,
    /// `?>`, including one newline directly after it.
    CloseTag,

    Identifier,
    /// `Foo\Bar`
    QualifiedName,
    /// `\Foo\Bar`
    FullyQualifiedName,
    /// `namespace\Foo`
    RelativeName,
    Variable,

    Integer,
    Float,
    /// A string literal without interpolation, quotes included.
    String,

    Abstract,
    And,
    Array,
    As,
    Break,
    Callable,
    Case,
    Catch,
    Class,
    Clone,
    Const,
    Continue,
    Declare,
    Default,
    Do,
    Echo,
    Else,
    ElseIf,
    Empty,
    EndDeclare,
    EndFor,
    EndForeach,
    EndIf,
    EndSwitch,
    EndWhile,
    Eval,
    /// `exit` or `die`
    Exit,
    Extends,
    Final,
    Finally,
    Fn,
    For,
    Foreach,
    Function,
    Global,
    Goto,
    If,
    Implements,
    Include,
    IncludeOnce,
    InstanceOf,
    InsteadOf,
    Interface,
    Isset,
    List,
    Match,
    Namespace,
    New,
    Or,
    Print,
    Private,
    Protected,
    Public,
    Readonly,
    Require,
    RequireOnce,
    Return,
    Static,
    Switch,
    Throw,
    Trait,
    Try,
    Unset,
    Use,
    Var,
    While,
    Xor,
    Yield,

    True,
    False,
    Null,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// `**`
    Power,
    Dot,
    /// `++`
    Increment,
    /// `--`
    Decrement,

    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    PowerAssign,
    DotAssign,
    NullCoalesceAssign,
    AmpersandAssign,
    PipeAssign,
    CaretAssign,
    ShiftLeftAssign,
    ShiftRightAssign,

    /// `==`
    Equal,
    /// `===`
    Identical,
    /// `!=` or `<>`
    NotEqual,
    /// `!==`
    NotIdentical,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    /// `<=>`
    Spaceship,

    /// `!`
    Not,
    /// `&&`
    LogicalAnd,
    /// `||`
    LogicalOr,

    Ampersand,
    Pipe,
    Caret,
    Tilde,
    ShiftLeft,
    ShiftRight,

    /// `->`
    Arrow,
    /// `?->`
    NullsafeArrow,
    /// `::`
    DoubleColon,
    /// `=>`
    FatArrow,
    /// `??`
    NullCoalesce,
    /// `|>`
    PipeForward,
    Question,
    /// `@`
    At,
    /// `$`, when not starting a variable name, as in `$$name`.
    Dollar,
    /// `\` on its own.
    Backslash,
    /// `...`
    Ellipsis,
    /// `#[`
    AttributeStart,

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
