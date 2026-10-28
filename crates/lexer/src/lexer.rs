use phera_source::Span;

use crate::{LexError, LexErrorKind, Token, TokenKind};

/// Operators and punctuation, longest first so that `===` wins over `==`.
const OPERATORS: &[(&str, TokenKind)] = {
    use TokenKind::*;

    &[
        ("<=>", Spaceship),
        ("**=", PowerAssign),
        ("...", Ellipsis),
        ("<<=", ShiftLeftAssign),
        (">>=", ShiftRightAssign),
        ("===", Identical),
        ("!==", NotIdentical),
        ("??=", NullCoalesceAssign),
        ("?->", NullsafeArrow),
        ("++", Increment),
        ("--", Decrement),
        ("->", Arrow),
        ("=>", FatArrow),
        ("::", DoubleColon),
        ("??", NullCoalesce),
        ("==", Equal),
        ("!=", NotEqual),
        ("<>", NotEqual),
        ("<=", LessEqual),
        (">=", GreaterEqual),
        ("&&", LogicalAnd),
        ("||", LogicalOr),
        ("<<", ShiftLeft),
        (">>", ShiftRight),
        ("**", Power),
        ("+=", PlusAssign),
        ("-=", MinusAssign),
        ("*=", StarAssign),
        ("/=", SlashAssign),
        (".=", DotAssign),
        ("%=", PercentAssign),
        ("&=", AmpersandAssign),
        ("|=", PipeAssign),
        ("^=", CaretAssign),
        ("|>", PipeForward),
        ("#[", AttributeStart),
        ("+", Plus),
        ("-", Minus),
        ("*", Star),
        ("/", Slash),
        ("%", Percent),
        (".", Dot),
        ("=", Assign),
        ("!", Not),
        ("<", Less),
        (">", Greater),
        ("&", Ampersand),
        ("|", Pipe),
        ("^", Caret),
        ("~", Tilde),
        ("?", Question),
        ("@", At),
        ("$", Dollar),
        ("\\", Backslash),
        (":", Colon),
        (";", Semicolon),
        (",", Comma),
        ("(", LeftParen),
        (")", RightParen),
        ("{", LeftBrace),
        ("}", RightBrace),
        ("[", LeftBracket),
        ("]", RightBracket),
    ]
};

pub struct Lexer<'src> {
    source: &'src str,
    position: usize,
    /// Whether we're inside `<?php ... ?>`, rather than in inline HTML.
    in_php: bool,
}

impl<'src> Lexer<'src> {
    /// Creates a lexer for a PHP file, which starts as inline HTML until the
    /// first `<?php` or `<?=` tag.
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            position: 0,
            in_php: false,
        }
    }

    /// Creates a lexer for PHP code without an opening tag, such as the code
    /// passed to `eval()`.
    pub fn new_php(source: &'src str) -> Self {
        Self {
            in_php: true,
            ..Self::new(source)
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        if self.in_php {
            self.lex_php()
        } else {
            Ok(self.lex_html())
        }
    }

    fn lex_html(&mut self) -> Token {
        let start = self.position;

        if self.is_eof() {
            return self.token(TokenKind::Eof, start);
        }

        let Some((tag, kind, length)) = self.find_open_tag() else {
            self.position = self.source.len();
            return self.token(TokenKind::InlineHtml, start);
        };

        if tag > start {
            self.position = tag;
            return self.token(TokenKind::InlineHtml, start);
        }

        self.position += length;
        self.in_php = true;
        self.token(kind, start)
    }

    /// Finds the next `<?php` or `<?=` tag, returning its offset, kind and length.
    fn find_open_tag(&self) -> Option<(usize, TokenKind, usize)> {
        let bytes = self.source.as_bytes();
        let mut from = self.position;

        while let Some(index) = self.source[from..].find("<?") {
            let tag = from + index;
            let after = &bytes[tag + 2..];

            if after.starts_with(b"=") {
                return Some((tag, TokenKind::OpenTagWithEcho, 3));
            }

            // `<?php` must be followed by whitespace or the end of the file.
            if after.len() >= 3
                && after[..3].eq_ignore_ascii_case(b"php")
                && after.get(3).is_none_or(|b| b" \t\r\n".contains(b))
            {
                return Some((tag, TokenKind::OpenTag, 5));
            }

            from = tag + 2;
        }

        None
    }

    fn lex_php(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;

        let start = self.position;

        let Some(c) = self.peek() else {
            return Ok(self.token(TokenKind::Eof, start));
        };
        let next = self.peek_next();

        let kind = if self.starts_with("?>") {
            self.lex_close_tag()
        } else if c == '$' && next.is_some_and(is_name_start) {
            self.advance();
            self.eat_name();
            TokenKind::Variable
        } else if c.is_ascii_digit() || (c == '.' && next.is_some_and(|n| n.is_ascii_digit())) {
            self.lex_number()
        } else if c == '\\' && next.is_some_and(is_name_start) {
            self.advance();
            self.eat_qualified_name();
            TokenKind::FullyQualifiedName
        } else if is_name_start(c) {
            self.lex_name()
        } else if c == '\'' || c == '"' {
            self.lex_string()?
        } else if self.starts_with("<<<") {
            return self.unsupported(start, "heredoc and nowdoc strings");
        } else if let Some(&(operator, kind)) =
            OPERATORS.iter().find(|(op, _)| self.starts_with(op))
        {
            self.position += operator.len();
            kind
        } else {
            self.advance();
            return Err(LexError {
                kind: LexErrorKind::UnexpectedCharacter(c),
                span: Span::new(start, self.position),
            });
        };

        Ok(self.token(kind, start))
    }

    /// Lexes `?>`, which also swallows one directly following newline.
    fn lex_close_tag(&mut self) -> TokenKind {
        self.position += 2;

        if self.starts_with("\r\n") {
            self.position += 2;
        } else if self.starts_with("\n") {
            self.position += 1;
        }

        self.in_php = false;
        TokenKind::CloseTag
    }

    /// Lexes an identifier, keyword, or qualified name like `Foo\Bar`.
    fn lex_name(&mut self) -> TokenKind {
        let start = self.position;
        self.eat_name();

        let name = self.source[start..self.position].to_ascii_lowercase();

        if self.starts_qualified_part() {
            self.eat_qualified_name();

            return if name == "namespace" {
                TokenKind::RelativeName
            } else {
                TokenKind::QualifiedName
            };
        }

        keyword(&name).unwrap_or(TokenKind::Identifier)
    }

    /// Lexes integers in decimal, hex (`0x`), octal (`0o`) and binary (`0b`),
    /// and floats like `1.5`, `.5`, `1.` and `1e3`. Digits may be separated by `_`.
    fn lex_number(&mut self) -> TokenKind {
        let radix = match self.source.as_bytes()[self.position..] {
            [b'0', b'x' | b'X', ..] => 16,
            [b'0', b'o' | b'O', ..] => 8,
            [b'0', b'b' | b'B', ..] => 2,
            _ => 10,
        };

        // A prefix only counts if a digit follows, so `0x` is `0` then `x`.
        let after_prefix = self.source[self.position..].chars().nth(2);

        if radix != 10 && after_prefix.is_some_and(|c| c.is_digit(radix)) {
            self.position += 2;
            self.eat_digits(radix);
            return TokenKind::Integer;
        }

        self.eat_digits(10);
        let mut kind = TokenKind::Integer;

        if self.peek() == Some('.') {
            self.advance();
            self.eat_digits(10);
            kind = TokenKind::Float;
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            let exponent = &self.source[self.position + 1..];
            let sign = usize::from(exponent.starts_with(['+', '-']));

            if exponent[sign..].starts_with(|c: char| c.is_ascii_digit()) {
                self.position += 1 + sign;
                self.eat_digits(10);
                kind = TokenKind::Float;
            }
        }

        kind
    }

    /// Consumes digits in `radix`, allowing single `_` separators between them.
    fn eat_digits(&mut self, radix: u32) {
        while self.peek().is_some_and(|c| c.is_digit(radix)) {
            self.advance();

            if self.peek() == Some('_') && self.peek_next().is_some_and(|c| c.is_digit(radix)) {
                self.advance();
            }
        }
    }

    fn lex_string(&mut self) -> Result<TokenKind, LexError> {
        let start = self.position;
        let quote = self.advance().unwrap();

        while let Some(c) = self.peek() {
            if c == quote {
                self.advance();
                return Ok(TokenKind::String);
            }

            if c == '\\' {
                self.advance();
                self.advance();
                continue;
            }

            if quote == '"' && self.starts_interpolation() {
                return self.unsupported(start, "string interpolation");
            }

            self.advance();
        }

        Err(LexError {
            kind: LexErrorKind::UnterminatedString,
            span: Span::new(start, self.position),
        })
    }

    /// Whether a double-quoted string continues with `$name` or `{$`.
    fn starts_interpolation(&self) -> bool {
        self.starts_with("{$")
            || (self.peek() == Some('$') && self.peek_next().is_some_and(is_name_start))
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        loop {
            if self.peek().is_some_and(is_whitespace) {
                self.advance();
            } else if self.starts_with("//") || (self.starts_with("#") && !self.starts_with("#[")) {
                // Line comments end at a newline or a `?>` tag.
                while !self.is_eof() && !self.starts_with("\n") && !self.starts_with("?>") {
                    self.advance();
                }
            } else if self.starts_with("/*") {
                let start = self.position;

                match self.source[start + 2..].find("*/") {
                    Some(index) => self.position = start + 2 + index + 2,
                    None => {
                        self.position = self.source.len();

                        return Err(LexError {
                            kind: LexErrorKind::UnterminatedComment,
                            span: Span::new(start, self.position),
                        });
                    }
                }
            } else {
                return Ok(());
            }
        }
    }

    fn eat_name(&mut self) {
        while self.peek().is_some_and(is_name_char) {
            self.advance();
        }
    }

    fn starts_qualified_part(&self) -> bool {
        self.peek() == Some('\\') && self.peek_next().is_some_and(is_name_start)
    }

    /// Consumes a name like `Foo\Bar\Baz`.
    fn eat_qualified_name(&mut self) {
        self.eat_name();

        while self.starts_qualified_part() {
            self.advance();
            self.eat_name();
        }
    }

    fn token(&self, kind: TokenKind, start: usize) -> Token {
        Token {
            kind,
            span: Span::new(start, self.position),
        }
    }

    fn unsupported<T>(&self, start: usize, feature: &'static str) -> Result<T, LexError> {
        Err(LexError {
            kind: LexErrorKind::Unsupported(feature),
            span: Span::new(start, self.position),
        })
    }

    fn starts_with(&self, value: &str) -> bool {
        self.source[self.position..].starts_with(value)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.position..].chars();

        chars.next()?;
        chars.next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        Some(c)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }
}

/// PHP only treats these four characters as whitespace.
fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// Names may start with a letter, `_`, or any non-ASCII character.
fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || !c.is_ascii()
}

fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit()
}

/// Looks up a keyword. Keywords are case-insensitive, so `name` must be lowercase.
pub fn keyword(name: &str) -> Option<TokenKind> {
    use TokenKind::*;

    let kind = match name {
        "abstract" => Abstract,
        "and" => And,
        "array" => Array,
        "as" => As,
        "break" => Break,
        "callable" => Callable,
        "case" => Case,
        "catch" => Catch,
        "class" => Class,
        "clone" => Clone,
        "const" => Const,
        "continue" => Continue,
        "declare" => Declare,
        "default" => Default,
        "die" | "exit" => Exit,
        "do" => Do,
        "echo" => Echo,
        "else" => Else,
        "elseif" => ElseIf,
        "empty" => Empty,
        "enddeclare" => EndDeclare,
        "endfor" => EndFor,
        "endforeach" => EndForeach,
        "endif" => EndIf,
        "endswitch" => EndSwitch,
        "endwhile" => EndWhile,
        "eval" => Eval,
        "extends" => Extends,
        "final" => Final,
        "finally" => Finally,
        "fn" => Fn,
        "for" => For,
        "foreach" => Foreach,
        "function" => Function,
        "global" => Global,
        "goto" => Goto,
        "if" => If,
        "implements" => Implements,
        "include" => Include,
        "include_once" => IncludeOnce,
        "instanceof" => InstanceOf,
        "insteadof" => InsteadOf,
        "interface" => Interface,
        "isset" => Isset,
        "list" => List,
        "match" => Match,
        "namespace" => Namespace,
        "new" => New,
        "or" => Or,
        "print" => Print,
        "private" => Private,
        "protected" => Protected,
        "public" => Public,
        "readonly" => Readonly,
        "require" => Require,
        "require_once" => RequireOnce,
        "return" => Return,
        "static" => Static,
        "switch" => Switch,
        "throw" => Throw,
        "trait" => Trait,
        "try" => Try,
        "unset" => Unset,
        "use" => Use,
        "var" => Var,
        "while" => While,
        "xor" => Xor,
        "yield" => Yield,
        "true" => True,
        "false" => False,
        "null" => Null,
        _ => return None,
    };

    Some(kind)
}
