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
    /// A string literal without interpolation, quotes (and any `b` prefix)
    /// included: `'...'`, `"..."`, `b'...'`.
    String,

    // Interpolated strings are lexed like PHP does it: a `DoubleQuote` (or
    // `StartHeredoc`), then literal `StringPart`s mixed with the tokens of the
    // interpolated expressions, then a closing `DoubleQuote` (or `EndHeredoc`).
    /// `"` opening or closing an interpolated string; `b"` when opening.
    DoubleQuote,
    /// Literal text inside an interpolated string, heredoc or nowdoc.
    StringPart,
    /// `<<<EOT`, `<<<"EOT"` or `<<<'EOT'`, including the newline after it.
    StartHeredoc,
    /// The closing label of a heredoc or nowdoc, including its indentation.
    EndHeredoc,
    /// `{` starting a `{$...}` interpolation.
    CurlyOpen,
    /// A bare number used as an offset in `"$array[0]"`.
    NumString,

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
    /// `yield from`, including whatever whitespace or comments sit in between.
    YieldFrom,
    /// `enum`, only when it starts an enum declaration.
    Enum,
    /// `__halt_compiler`: everything after the `();` that follows is raw data.
    HaltCompiler,
    /// `public(set)`
    PublicSet,
    /// `protected(set)`
    ProtectedSet,
    /// `private(set)`
    PrivateSet,

    True,
    False,
    Null,

    /// `__DIR__`
    MagicDir,
    /// `__FILE__`
    MagicFile,
    /// `__LINE__`
    MagicLine,
    /// `__CLASS__`
    MagicClass,
    /// `__FUNCTION__`
    MagicFunction,
    /// `__METHOD__`
    MagicMethod,
    /// `__NAMESPACE__`
    MagicNamespace,
    /// `__TRAIT__`
    MagicTrait,
    /// `__PROPERTY__`
    MagicProperty,

    // Casts are single tokens, as in PHP: `(int)`, `( string )`, `(Bool)`.
    /// `(int)` or `(integer)`
    IntCast,
    /// `(float)` or `(double)`
    FloatCast,
    /// `(string)` or `(binary)`
    StringCast,
    /// `(array)`
    ArrayCast,
    /// `(object)`
    ObjectCast,
    /// `(bool)` or `(boolean)`
    BoolCast,
    /// `(unset)`
    UnsetCast,
    /// `(void)`
    VoidCast,

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

    // Tokens only returned by the lossless tokenizer scanner.
    Whitespace,
    Comment,
    DocComment,
    BadCharacter,
    Backquote,
    DollarCurlyOpen,
    StringVarname,
    AmpersandBeforeVariable,
    OffsetAmpersand,

    Eof,
}
