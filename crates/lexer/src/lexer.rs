//! The lexer proper.
//!
//! PHP can't be lexed with a single set of rules: what `$a` or `}` means
//! depends on whether we're in code, inside a `"..."` string, in a heredoc, or
//! just after `$a[` inside one of those. Like PHP's own scanner, the lexer
//! keeps a stack of [`Mode`]s and dispatches on the top one. Inline HTML and
//! PHP code replace the top of the stack; `{$` inside a string pushes a new
//! PHP mode that the matching `}` pops again.
//!
//! The token stream for strings matches `token_get_all()` exactly, so that
//! the comparison harness in `crates/phpt` can check every file in php-src.

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

/// What the lexer is currently inside of; see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Text outside `<?php ... ?>`.
    Html,
    /// PHP code.
    Php,
    /// Inside an interpolated `"..."` string.
    DoubleQuotes,
    /// Inside a heredoc body; `label` is where its label sits in the source.
    Heredoc { label: Span },
    /// Inside a nowdoc body: like a heredoc, but nothing is interpolated.
    Nowdoc { label: Span },
    /// Just after `$name[` inside a string: lexing the offset up to `]`.
    VarOffset,
    /// Just after `$name` followed by `->name` inside a string.
    Property,
    /// After `__halt_compiler();`: the rest of the file is raw data.
    Halted,
}

pub struct Lexer<'src> {
    source: &'src [u8],
    position: usize,
    /// The mode stack; never empty.
    modes: Vec<Mode>,
    /// After `__halt_compiler`, how many more tokens (`(`, `)`, `;`) to lex
    /// before the rest of the file becomes raw data.
    halt_countdown: Option<u8>,
    /// The kind of the previous token in PHP code, to tell `__halt_compiler`
    /// apart from `$obj->__halt_compiler`.
    previous: TokenKind,
}

impl<'src> Lexer<'src> {
    /// Creates a lexer for a PHP file, which starts as inline HTML until the
    /// first `<?php` or `<?=` tag.
    pub fn new(source: &'src str) -> Self {
        Self {
            source: source.as_bytes(),
            position: 0,
            modes: vec![Mode::Html],
            halt_countdown: None,
            previous: TokenKind::Eof,
        }
    }

    /// Creates a lexer for PHP code without an opening tag, such as the code
    /// passed to `eval()`.
    pub fn new_php(source: &'src str) -> Self {
        Self {
            modes: vec![Mode::Php],
            ..Self::new(source)
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        match self.mode() {
            Mode::Html => Ok(self.lex_html()),
            Mode::Php => self.lex_php_counting_halt(),
            Mode::DoubleQuotes => self.lex_double_quotes(),
            Mode::Heredoc { label } => self.lex_heredoc_body(label, true),
            Mode::Nowdoc { label } => self.lex_heredoc_body(label, false),
            Mode::VarOffset => self.lex_var_offset(),
            Mode::Property => Ok(self.lex_property()),
            Mode::Halted => Ok(self.lex_halted()),
        }
    }

    // --- Modes ---------------------------------------------------------------

    fn mode(&self) -> Mode {
        *self.modes.last().expect("mode stack is never empty")
    }

    /// Replaces the current mode, like `BEGIN` in PHP's scanner.
    fn begin(&mut self, mode: Mode) {
        *self.modes.last_mut().expect("mode stack is never empty") = mode;
    }

    fn push(&mut self, mode: Mode) {
        self.modes.push(mode);
    }

    /// Pops a pushed mode, never the bottom one.
    fn pop(&mut self) {
        if self.modes.len() > 1 {
            self.modes.pop();
        }
    }

    // --- Inline HTML -----------------------------------------------------------

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
        self.begin(Mode::Php);
        self.token(kind, start)
    }

    /// Finds the next `<?php` or `<?=` tag, returning its offset, kind and length.
    fn find_open_tag(&self) -> Option<(usize, TokenKind, usize)> {
        let bytes = self.source;
        let mut from = self.position;

        while let Some(index) = find_bytes(&self.source[from..], b"<?") {
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

    /// After `__halt_compiler();` the rest of the file is one chunk of data.
    fn lex_halted(&mut self) -> Token {
        let start = self.position;

        if self.is_eof() {
            return self.token(TokenKind::Eof, start);
        }

        self.position = self.source.len();
        self.token(TokenKind::InlineHtml, start)
    }

    // --- PHP code --------------------------------------------------------------

    /// Lexes PHP code, and after `__halt_compiler` stops once its `();` is done,
    /// like `token_get_all()` does.
    fn lex_php_counting_halt(&mut self) -> Result<Token, LexError> {
        let token = self.lex_php()?;
        let member_name = matches!(self.previous, TokenKind::Arrow | TokenKind::NullsafeArrow);
        self.previous = token.kind;

        if token.kind == TokenKind::HaltCompiler && !member_name {
            self.halt_countdown = Some(3);
        } else if let Some(remaining) = &mut self.halt_countdown
            && token.kind != TokenKind::Eof
        {
            *remaining -= 1;

            if *remaining == 0 {
                self.halt_countdown = None;
                self.begin(Mode::Halted);
            }
        }

        Ok(token)
    }

    fn lex_php(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace_and_comments()?;

        let start = self.position;

        let Some(c) = self.peek() else {
            return self.eof_in_php(start);
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
        } else if matches!(c, 'b' | 'B') && matches!(next, Some('\'' | '"')) {
            // Binary string prefix, a leftover from PHP 6: `b"..."` is `"..."`.
            self.advance();
            self.lex_quoted_string(start)?
        } else if matches!(c, 'b' | 'B')
            && self.source[self.position + 1..].starts_with(b"<<<")
            && let Some(kind) = self.lex_heredoc_start(self.position + 1)
        {
            kind
        } else if is_name_start(c) {
            self.lex_name()
        } else if c == '\'' || c == '"' {
            self.lex_quoted_string(start)?
        } else if self.starts_with("<<<")
            && let Some(kind) = self.lex_heredoc_start(self.position)
        {
            kind
        } else if c == '('
            && let Some(kind) = self.lex_cast()?
        {
            kind
        } else if c == '{' {
            self.advance();
            self.push(Mode::Php);
            TokenKind::LeftBrace
        } else if c == '}' {
            self.advance();
            self.pop();
            TokenKind::RightBrace
        } else if c == '`' {
            self.advance();
            return self.unsupported(start, "backtick shell execution");
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

    /// The end of the file in PHP code, which is an error inside the `{$...}`
    /// of a string that was never closed.
    fn eof_in_php(&self, start: usize) -> Result<Token, LexError> {
        let unterminated = self.modes.iter().rev().find_map(|mode| match mode {
            Mode::DoubleQuotes => Some(LexErrorKind::UnterminatedString),
            Mode::Heredoc { .. } | Mode::Nowdoc { .. } => Some(LexErrorKind::UnterminatedHeredoc),
            _ => None,
        });

        match unterminated {
            Some(kind) => Err(LexError {
                kind,
                span: Span::new(start, start),
            }),
            None => Ok(self.token(TokenKind::Eof, start)),
        }
    }

    /// Lexes `?>`, which also swallows one directly following newline.
    fn lex_close_tag(&mut self) -> TokenKind {
        self.position += 2;

        if self.starts_with("\r\n") {
            self.position += 2;
        } else if matches!(self.peek(), Some('\r' | '\n')) {
            self.position += 1;
        }

        self.begin(Mode::Html);
        TokenKind::CloseTag
    }

    /// Lexes an identifier, keyword, or qualified name like `Foo\Bar`.
    fn lex_name(&mut self) -> TokenKind {
        let start = self.position;
        self.eat_name();

        let name = self.source[start..self.position].to_ascii_lowercase();

        if self.starts_qualified_part() {
            self.eat_qualified_name();

            return if name == b"namespace" {
                TokenKind::RelativeName
            } else {
                TokenKind::QualifiedName
            };
        }

        // A few keywords depend on what follows them.
        match name.as_slice() {
            // `enum` is only a keyword when it starts a declaration, so that
            // `enum` stays usable as a class or function name.
            b"enum" => {
                let after = self.trivia_end(self.position).unwrap_or(self.position);
                let following = self.name_at(after).to_ascii_lowercase();

                let declares = !following.is_empty()
                    && !following.starts_with(b"extends")
                    && !following.starts_with(b"implements");

                return if declares {
                    TokenKind::Enum
                } else {
                    TokenKind::Identifier
                };
            }
            // `yield from` is one token, even across whitespace and comments.
            b"yield" => {
                if let Ok(after) = self.trivia_end(self.position)
                    && self.name_at(after).eq_ignore_ascii_case(b"from")
                {
                    self.position = after + "from".len();
                    return TokenKind::YieldFrom;
                }
            }
            // Asymmetric visibility: `private(set)` with no whitespace inside.
            b"public" | b"protected" | b"private" if self.starts_with_ignore_case("(set)") => {
                self.position += "(set)".len();

                return match name.as_slice() {
                    b"public" => TokenKind::PublicSet,
                    b"protected" => TokenKind::ProtectedSet,
                    _ => TokenKind::PrivateSet,
                };
            }
            _ => {}
        }

        name_kind(&name)
    }

    /// The name starting at `position`, or `""` if there is none.
    fn name_at(&self, position: usize) -> &'src [u8] {
        let end = self.source[position..]
            .iter()
            .copied()
            .map(char::from)
            .enumerate()
            .find(|&(i, c)| {
                !(if i == 0 {
                    is_name_start(c)
                } else {
                    is_name_char(c)
                })
            })
            .map_or(self.source.len(), |(i, _)| position + i);

        &self.source[position..end]
    }

    /// Lexes a cast like `(int)` or `( string )`; `None` if `(` isn't one.
    fn lex_cast(&mut self) -> Result<Option<TokenKind>, LexError> {
        let start = self.position;
        let mut position = start + 1;

        let skip_blanks = |position: &mut usize| {
            while self
                .source
                .get(*position)
                .is_some_and(|b| b" \t".contains(b))
            {
                *position += 1;
            }
        };

        skip_blanks(&mut position);
        let word = self.name_at(position);
        position += word.len();
        skip_blanks(&mut position);

        if self.source.get(position) != Some(&b')') {
            return Ok(None);
        }

        let kind = match word.to_ascii_lowercase().as_slice() {
            b"int" | b"integer" => TokenKind::IntCast,
            b"float" | b"double" => TokenKind::FloatCast,
            b"string" | b"binary" => TokenKind::StringCast,
            b"array" => TokenKind::ArrayCast,
            b"object" => TokenKind::ObjectCast,
            b"bool" | b"boolean" => TokenKind::BoolCast,
            b"unset" => TokenKind::UnsetCast,
            b"void" => TokenKind::VoidCast,
            b"real" => {
                self.position = position + 1;
                return self.unsupported(start, "the (real) cast, removed in PHP 8");
            }
            _ => return Ok(None),
        };

        self.position = position + 1;
        Ok(Some(kind))
    }

    /// Lexes integers in decimal, hex (`0x`), octal (`0o`) and binary (`0b`),
    /// and floats like `1.5`, `.5`, `1.` and `1e3`. Digits may be separated by `_`.
    fn lex_number(&mut self) -> TokenKind {
        if self.eat_prefixed_integer() {
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
            let sign = usize::from(matches!(exponent.first(), Some(b'+' | b'-')));

            if exponent.get(sign).is_some_and(u8::is_ascii_digit) {
                self.position += 1 + sign;
                self.eat_digits(10);
                kind = TokenKind::Float;
            }
        }

        kind
    }

    /// Consumes a `0x`, `0o` or `0b` integer, if there is one here.
    fn eat_prefixed_integer(&mut self) -> bool {
        let radix = match self.source[self.position..] {
            [b'0', b'x' | b'X', ..] => 16,
            [b'0', b'o' | b'O', ..] => 8,
            [b'0', b'b' | b'B', ..] => 2,
            _ => return false,
        };

        // A prefix only counts if a digit follows, so `0x` is `0` then `x`.
        let after_prefix = self.source.get(self.position + 2).copied().map(char::from);

        if !after_prefix.is_some_and(|c| c.is_digit(radix)) {
            return false;
        }

        self.position += 2;
        self.eat_digits(radix);
        true
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

    // --- Strings ---------------------------------------------------------------

    /// Lexes a `'...'` or `"..."` string starting at the quote. `start` is where
    /// the token began, which is one byte earlier for a `b` prefix.
    ///
    /// A double-quoted string with nothing interpolated is one `String` token.
    /// With interpolation, only the opening `"` is lexed here, and the lexer
    /// switches to [`Mode::DoubleQuotes`] for the rest.
    fn lex_quoted_string(&mut self, start: usize) -> Result<TokenKind, LexError> {
        let quote = self.advance().expect("called at a quote");
        let body = self.position;

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
                self.position = body;
                self.begin(Mode::DoubleQuotes);
                return Ok(TokenKind::DoubleQuote);
            }

            self.advance();
        }

        Err(LexError {
            kind: LexErrorKind::UnterminatedString,
            span: Span::new(start, self.position),
        })
    }

    /// Whether an interpolated string continues with `$name`, `${` or `{$`.
    fn starts_interpolation(&self) -> bool {
        self.starts_with("{$")
            || (self.peek() == Some('$')
                && self
                    .peek_next()
                    .is_some_and(|n| n == '{' || is_name_start(n)))
    }

    /// Lexes inside an interpolated `"..."` string: literal parts, interpolated
    /// variables, and the closing quote.
    fn lex_double_quotes(&mut self) -> Result<Token, LexError> {
        let start = self.position;

        if self.peek() == Some('"') {
            self.advance();
            self.begin(Mode::Php);
            return Ok(self.token(TokenKind::DoubleQuote, start));
        }

        if let Some(token) = self.lex_interpolation()? {
            return Ok(token);
        }

        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        kind: LexErrorKind::UnterminatedString,
                        span: Span::new(start, self.position),
                    });
                }
                Some('"') => break,
                Some('\\') => {
                    self.advance();
                    self.advance();
                }
                Some('$' | '{') if self.starts_interpolation() => break,
                Some(_) => {
                    self.advance();
                }
            }
        }

        Ok(self.token(TokenKind::StringPart, start))
    }

    /// Lexes what interpolation inside a string starts with: `$name` (which may
    /// continue as `$name[...]` or `$name->prop`) or `{$`. `None` if the string
    /// continues with literal text.
    fn lex_interpolation(&mut self) -> Result<Option<Token>, LexError> {
        let start = self.position;

        if !self.starts_interpolation() {
            return Ok(None);
        }

        if self.starts_with("{$") {
            self.advance();
            self.push(Mode::Php);
            return Ok(Some(self.token(TokenKind::CurlyOpen, start)));
        }

        if self.starts_with("${") {
            self.position += 2;
            return self.unsupported(
                start,
                "`${...}` string interpolation (deprecated in PHP 8.2)",
            );
        }

        self.advance();
        self.eat_name();

        // What may follow a variable inside a string. PHP's scanner decides
        // this when it lexes the variable, so `"$a->"` stays literal text.
        let rest = &self.source[self.position..];
        let property_follows = |arrow: &str| {
            rest.strip_prefix(arrow.as_bytes())
                .and_then(|after| after.first())
                .is_some_and(|&byte| is_name_start(char::from(byte)))
        };

        if rest.starts_with(b"[") {
            self.push(Mode::VarOffset);
        } else if property_follows("->") || property_follows("?->") {
            self.push(Mode::Property);
        }

        Ok(Some(self.token(TokenKind::Variable, start)))
    }

    /// Lexes the offset in `"$array[...]"`: a number, a bare name, or another
    /// variable, with `-` allowed before a number.
    fn lex_var_offset(&mut self) -> Result<Token, LexError> {
        let start = self.position;

        let Some(c) = self.peek() else {
            return Err(LexError {
                kind: LexErrorKind::UnterminatedString,
                span: Span::new(start, start),
            });
        };

        let kind = match c {
            '[' => {
                self.advance();
                TokenKind::LeftBracket
            }
            ']' => {
                self.advance();
                self.pop();
                TokenKind::RightBracket
            }
            '-' => {
                self.advance();
                TokenKind::Minus
            }
            '$' if self.peek_next().is_some_and(is_name_start) => {
                self.advance();
                self.eat_name();
                TokenKind::Variable
            }
            _ if c.is_ascii_digit() => {
                if !self.eat_prefixed_integer() {
                    self.eat_digits(10);
                }
                TokenKind::NumString
            }
            _ if is_name_start(c) => {
                self.eat_name();
                let name = self.source[start..self.position].to_ascii_lowercase();
                name_kind(&name)
            }
            _ => {
                self.advance();
                return Err(LexError {
                    kind: LexErrorKind::UnexpectedCharacter(c),
                    span: Span::new(start, self.position),
                });
            }
        };

        Ok(self.token(kind, start))
    }

    /// Lexes `->name` or `?->name` after a variable inside a string. The mode
    /// is only entered when exactly that follows, so this can't fail.
    fn lex_property(&mut self) -> Token {
        let start = self.position;

        let kind = if self.starts_with("->") {
            self.position += 2;
            TokenKind::Arrow
        } else if self.starts_with("?->") {
            self.position += 3;
            TokenKind::NullsafeArrow
        } else {
            self.eat_name();
            self.pop();
            let name = self.source[start..self.position].to_ascii_lowercase();
            name_kind(&name)
        };

        self.token(kind, start)
    }

    // --- Heredocs and nowdocs ----------------------------------------------------

    /// Lexes `<<<LABEL`, `<<<"LABEL"` or `<<<'LABEL'` plus the newline after it,
    /// starting at the `<<<` at `from`. `None` if that isn't how the text continues.
    fn lex_heredoc_start(&mut self, from: usize) -> Option<TokenKind> {
        let bytes = self.source;
        let mut position = from + 3;

        while bytes.get(position).is_some_and(|b| b" \t".contains(b)) {
            position += 1;
        }

        let quote = bytes.get(position).filter(|b| b"'\"".contains(b)).copied();
        if quote.is_some() {
            position += 1;
        }

        let label = self.name_at(position);
        if label.is_empty() {
            return None;
        }
        let label = Span::new(position, position + label.len());
        position = label.end;

        if let Some(quote) = quote {
            if bytes.get(position) != Some(&quote) {
                return None;
            }
            position += 1;
        }

        match &bytes[position..] {
            [b'\r', b'\n', ..] => position += 2,
            [b'\n' | b'\r', ..] => position += 1,
            _ => return None,
        }

        self.position = position;
        self.begin(if quote == Some(b'\'') {
            Mode::Nowdoc { label }
        } else {
            Mode::Heredoc { label }
        });

        Some(TokenKind::StartHeredoc)
    }

    /// Lexes inside a heredoc or nowdoc body: literal parts, interpolation
    /// (heredocs only), and the closing label.
    ///
    /// The body ends at the first line that holds only the label, possibly
    /// indented. The newline before that line belongs to the last literal part,
    /// and the indentation belongs to the `EndHeredoc` token.
    fn lex_heredoc_body(&mut self, label: Span, interpolate: bool) -> Result<Token, LexError> {
        let start = self.position;

        if self.at_line_start()
            && let Some(end) = self.closing_label_end(label)
        {
            self.position = end;
            self.begin(Mode::Php);
            return Ok(self.token(TokenKind::EndHeredoc, start));
        }

        if interpolate && let Some(token) = self.lex_interpolation()? {
            return Ok(token);
        }

        loop {
            match self.peek() {
                None => {
                    return Err(LexError {
                        kind: LexErrorKind::UnterminatedHeredoc,
                        span: Span::new(start, self.position),
                    });
                }
                Some('\n' | '\r') => {
                    if self.starts_with("\r\n") {
                        self.advance();
                    }
                    self.advance();

                    if self.closing_label_end(label).is_some() {
                        break;
                    }
                }
                // A backslash escapes the next character, except a newline:
                // the closing label still counts after `\` at a line's end.
                Some('\\') if interpolate => {
                    self.advance();

                    if !matches!(self.peek(), Some('\n' | '\r')) {
                        self.advance();
                    }
                }
                Some('$' | '{') if interpolate && self.starts_interpolation() => break,
                Some(_) => {
                    self.advance();
                }
            }
        }

        Ok(self.token(TokenKind::StringPart, start))
    }

    fn at_line_start(&self) -> bool {
        self.position == 0 || matches!(self.source[self.position - 1], b'\n' | b'\r')
    }

    /// If the current line is `label` after optional indentation, returns where
    /// the label ends. The label must be followed by something that can't
    /// continue a name, so `EOT;` closes but `EOTX` doesn't, and neither does
    /// `EOT` at the very end of the file.
    fn closing_label_end(&self, label: Span) -> Option<usize> {
        let bytes = self.source;
        let mut position = self.position;

        while bytes.get(position).is_some_and(|b| b" \t".contains(b)) {
            position += 1;
        }

        let end = position + (label.end - label.start);

        if bytes.get(position..end) != Some(&bytes[label.start..label.end]) {
            return None;
        }

        let closed = self
            .source
            .get(end)
            .copied()
            .map(char::from)
            .is_some_and(|c| !is_name_char(c));

        closed.then_some(end)
    }

    // --- Whitespace and comments -----------------------------------------------

    fn skip_whitespace_and_comments(&mut self) -> Result<(), LexError> {
        self.position = self.trivia_end(self.position)?;
        Ok(())
    }

    /// Where the whitespace and comments starting at `from` end.
    fn trivia_end(&self, from: usize) -> Result<usize, LexError> {
        let source = self.source;
        let mut position = from;

        loop {
            let rest = &source[position..];

            if rest
                .first()
                .is_some_and(|&byte| is_whitespace(char::from(byte)))
            {
                position += 1;
            } else if rest.starts_with(b"//")
                || (rest.starts_with(b"#") && !rest.starts_with(b"#["))
            {
                // Line comments end at a newline or a `?>` tag.
                let end = rest
                    .iter()
                    .copied()
                    .map(char::from)
                    .enumerate()
                    .find(|&(i, c)| matches!(c, '\n' | '\r') || rest[i..].starts_with(b"?>"))
                    .map_or(rest.len(), |(i, _)| i);
                position += end;
            } else if let Some(comment) = rest.strip_prefix(b"/*") {
                match find_bytes(comment, b"*/") {
                    Some(index) => position += 2 + index + 2,
                    None => {
                        return Err(LexError {
                            kind: LexErrorKind::UnterminatedComment,
                            span: Span::new(position, source.len()),
                        });
                    }
                }
            } else {
                return Ok(position);
            }
        }
    }

    // --- Cursor helpers ----------------------------------------------------------

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
        self.source[self.position..].starts_with(value.as_bytes())
    }

    fn starts_with_ignore_case(&self, value: &str) -> bool {
        self.source[self.position..]
            .get(..value.len())
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(value.as_bytes()))
    }

    fn peek(&self) -> Option<char> {
        self.source.get(self.position).copied().map(char::from)
    }

    fn peek_next(&self) -> Option<char> {
        self.source.get(self.position + 1).copied().map(char::from)
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += 1;
        Some(c)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }
}

fn find_bytes(source: &[u8], pattern: &[u8]) -> Option<usize> {
    source
        .windows(pattern.len())
        .position(|bytes| bytes == pattern)
}

fn name_kind(lowercase_name: &[u8]) -> TokenKind {
    std::str::from_utf8(lowercase_name)
        .ok()
        .and_then(keyword)
        .unwrap_or(TokenKind::Identifier)
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
///
/// `enum` and `yield from` aren't here: whether they're keywords depends on
/// what follows them, which only the lexer knows.
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
        "__halt_compiler" => HaltCompiler,
        "true" => True,
        "false" => False,
        "null" => Null,
        "__dir__" => MagicDir,
        "__file__" => MagicFile,
        "__line__" => MagicLine,
        "__class__" => MagicClass,
        "__function__" => MagicFunction,
        "__method__" => MagicMethod,
        "__namespace__" => MagicNamespace,
        "__trait__" => MagicTrait,
        "__property__" => MagicProperty,
        _ => return None,
    };

    Some(kind)
}
