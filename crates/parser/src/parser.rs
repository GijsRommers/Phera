//! The parser's shared mechanics: a pre-lexed token buffer and the small
//! helpers every parsing routine uses.
//!
//! The whole file is lexed up front. That keeps a two-token lookahead trivial
//! and lets `__halt_compiler` hand the rest of the file back as one raw
//! `InlineHtml` token, exactly like the lexer produces it.

use std::borrow::Cow;

use phera_ast::{Program, Statement, StatementKind};
use phera_lexer::{LexError, Lexer, ScanMode, ScanReport, Token, TokenKind};
use phera_source::Span;

use crate::error::ParseError;
use crate::nesting::Nesting;

pub(crate) struct Parser<'src> {
    pub(crate) source: &'src [u8],
    tokens: Vec<Token>,
    doc_comments: Vec<(usize, Span)>,
    pos: usize,
    /// The lexer error that stopped lexing early, if any. A synthetic `Eof`
    /// token sits at the end of `tokens` in that case.
    lex_error: Option<LexError>,
    /// End offset of the last consumed token, for building spans.
    pub(crate) prev_end: usize,
    pub(crate) nesting: Nesting,
    /// Set after `__halt_compiler();`: the rest of the file is raw data, so
    /// every open block is implicitly closed.
    pub(crate) halted: bool,
}

impl<'src> Parser<'src> {
    pub(crate) fn new(source: &'src [u8], collect_diagnostics: bool) -> (Self, ScanReport) {
        let lexer = Lexer::new_bytes(source).with_doc_comments();
        let mut lexer = if collect_diagnostics {
            lexer.with_diagnostics(ScanMode::Parsed)
        } else {
            lexer
        };
        let mut tokens = Vec::new();
        let mut lex_error = None;

        loop {
            match lexer.next_token() {
                Ok(token) => {
                    let eof = token.kind == TokenKind::Eof;
                    tokens.push(token);
                    if eof {
                        break;
                    }
                }
                Err(error) => {
                    lex_error = Some(error);
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        span: Span::new(0, 0),
                    });
                    break;
                }
            }
        }

        let parser = Self {
            source,
            tokens,
            doc_comments: lexer.take_doc_comments(),
            pos: 0,
            lex_error,
            prev_end: 0,
            nesting: Nesting::default(),
            halted: false,
        };
        (parser, lexer.into_diagnostics())
    }

    pub(crate) fn parse_program(mut self) -> Result<Program, ParseError> {
        let statements = self.parse_top_level(&[])?;
        if self.at(TokenKind::RightBrace) {
            return Err(self.error("unexpected '}'"));
        }
        Ok(Program { statements })
    }

    /// Parses top-level statements, stopping (without consuming) at any token
    /// in `stop` or at the end of the file.
    pub(crate) fn parse_top_level(
        &mut self,
        stop: &[TokenKind],
    ) -> Result<Vec<Statement>, ParseError> {
        let mut statements = Vec::new();

        loop {
            match self.kind() {
                TokenKind::Eof => {
                    // Lexing stopped early with an error: surface it.
                    if let Some(error) = &self.lex_error {
                        return Err(ParseError::from_lex(error));
                    }
                    break;
                }
                kind if stop.contains(&kind) => break,
                // A braced body (namespace, ...) ends here; at the true top
                // level `parse_program` reports the stray `}`.
                TokenKind::RightBrace => break,
                TokenKind::OpenTag | TokenKind::CloseTag | TokenKind::Semicolon => {
                    self.bump()?;
                }
                TokenKind::InlineHtml => {
                    let token = self.bump()?;
                    let html = self.bytes_of(&token).to_vec();
                    statements.push(Statement {
                        kind: StatementKind::InlineHtml(html),
                        span: token.span,
                    });
                }
                TokenKind::OpenTagWithEcho => {
                    statements.push(self.parse_echo()?);
                }
                TokenKind::Namespace => {
                    if stop.contains(&TokenKind::Namespace) {
                        break;
                    }
                    statements.push(self.parse_namespace()?);
                }
                TokenKind::HaltCompiler => {
                    statements.push(self.parse_halt_compiler()?);
                    // Everything after `__halt_compiler();` is raw data.
                    break;
                }
                _ => statements.push(self.parse_top_statement()?),
            }
        }

        Ok(statements)
    }

    // --- Token access --------------------------------------------------------

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn second(&self) -> &Token {
        &self.tokens[(self.pos + 1).min(self.tokens.len() - 1)]
    }

    pub(crate) fn second_span(&self) -> Span {
        self.second().span
    }

    /// The kind of the current token.
    pub(crate) fn kind(&self) -> TokenKind {
        self.current().kind
    }

    /// The kind of the token after the current one.
    pub(crate) fn kind2(&self) -> TokenKind {
        self.second().kind
    }

    /// The kind of the token two positions ahead.
    pub(crate) fn kind3(&self) -> TokenKind {
        self.tokens[(self.pos + 2).min(self.tokens.len() - 1)].kind
    }

    /// The kind of the token three positions ahead.
    pub(crate) fn kind4(&self) -> TokenKind {
        self.tokens[(self.pos + 3).min(self.tokens.len() - 1)].kind
    }

    /// Doc comment immediately before this token, if the scanner saw one.
    pub(crate) fn doc_comment(&self) -> Option<Span> {
        if self.doc_comments.is_empty() {
            return None;
        }
        self.doc_comment_before(self.peek_span().start)
    }

    pub(crate) fn doc_comment_before(&self, offset: usize) -> Option<Span> {
        let index = self
            .doc_comments
            .binary_search_by_key(&offset, |(start, _)| *start)
            .ok()?;
        self.doc_comments.get(index).map(|(_, comment)| *comment)
    }

    pub(crate) fn peek_span(&self) -> Span {
        self.current().span
    }

    /// The source text of the current token.
    pub(crate) fn text(&self) -> Cow<'src, str> {
        self.text_of(self.current())
    }

    pub(crate) fn text_of(&self, token: &Token) -> Cow<'src, str> {
        let bytes = self.bytes_of(token);
        match std::str::from_utf8(bytes) {
            Ok(text) => Cow::Borrowed(text),
            Err(_) => {
                // NUL cannot occur in a PHP name. Escape high bytes injectively
                // until bytecode and symbol tables can hold byte-string names.
                let mut name = String::new();
                for &byte in bytes {
                    if byte.is_ascii() {
                        name.push(char::from(byte));
                    } else {
                        use std::fmt::Write;
                        write!(name, "\0{byte:02x}").expect("string writes succeed");
                    }
                }
                Cow::Owned(name)
            }
        }
    }

    pub(crate) fn bytes_of(&self, token: &Token) -> &'src [u8] {
        &self.source[token.span.start..token.span.end]
    }

    pub(crate) fn at(&self, kind: TokenKind) -> bool {
        self.kind() == kind
    }

    pub(crate) fn at2(&self, kind: TokenKind) -> bool {
        self.kind2() == kind
    }

    /// Consumes the current token.
    pub(crate) fn bump(&mut self) -> Result<Token, ParseError> {
        let token = self.current().clone();

        if token.kind == TokenKind::Eof
            && let Some(error) = &self.lex_error
        {
            return Err(ParseError::from_lex(error));
        }

        self.prev_end = token.span.end;
        self.pos += 1;
        Ok(token)
    }

    /// Consumes the current token if it has the given kind.
    pub(crate) fn eat(&mut self, kind: TokenKind) -> Result<bool, ParseError> {
        if self.at(kind) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Consumes the current token, which must have the given kind.
    pub(crate) fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token, ParseError> {
        if self.at(kind) {
            self.bump()
        } else {
            Err(self.error(format!(
                "expected {what}, found {}",
                self.describe_current()
            )))
        }
    }

    // --- Errors ----------------------------------------------------------------

    /// An error at the current token. A lexer error that stopped lexing early
    /// takes precedence, since it is the real cause.
    pub(crate) fn error(&self, message: impl Into<String>) -> ParseError {
        if self.current().kind == TokenKind::Eof
            && let Some(error) = &self.lex_error
        {
            return ParseError::from_lex(error);
        }
        ParseError::new(message, self.current().span)
    }

    pub(crate) fn error_at(&self, span: Span, message: impl Into<String>) -> ParseError {
        ParseError::new(message, span)
    }

    /// A short description of the current token for error messages.
    pub(crate) fn describe_current(&self) -> String {
        let token = self.current();
        if token.kind == TokenKind::Eof {
            "end of file".to_string()
        } else {
            let text = String::from_utf8_lossy(self.bytes_of(token));
            let text = if text.chars().count() > 30 {
                format!("{}...", text.chars().take(30).collect::<String>())
            } else {
                text.to_string()
            };
            format!("token \"{text}\"")
        }
    }

    // --- Spans ------------------------------------------------------------------

    /// A span from `start` up to the end of the last consumed token.
    pub(crate) fn span_since(&self, start: Span) -> Span {
        Span::new(start.start, self.prev_end)
    }
}

/// Whether `kind` can be used as an identifier where PHP allows reserved
/// words: after `->` and `::`, as method/constant/enum-case names, and in
/// named arguments. Any keyword token is accepted; the source text is the name.
pub(crate) fn is_word_token(kind: TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        kind,
        Identifier
            | Abstract
            | And
            | Array
            | As
            | Break
            | Callable
            | Case
            | Catch
            | Class
            | Clone
            | Const
            | Continue
            | Declare
            | Default
            | Do
            | Echo
            | Else
            | ElseIf
            | Empty
            | EndDeclare
            | EndFor
            | EndForeach
            | EndIf
            | EndSwitch
            | EndWhile
            | Eval
            | Exit
            | Extends
            | Final
            | Finally
            | Fn
            | For
            | Foreach
            | Function
            | Global
            | Goto
            | If
            | Implements
            | Include
            | IncludeOnce
            | InstanceOf
            | InsteadOf
            | Interface
            | Isset
            | List
            | Match
            | Namespace
            | New
            | Or
            | Print
            | Private
            | Protected
            | Public
            | Readonly
            | Require
            | RequireOnce
            | Return
            | Static
            | Switch
            | Throw
            | Trait
            | Try
            | Unset
            | Use
            | Var
            | While
            | Xor
            | Yield
            | YieldFrom
            | Enum
            | HaltCompiler
            | PublicSet
            | ProtectedSet
            | PrivateSet
            | True
            | False
            | Null
            | MagicDir
            | MagicFile
            | MagicLine
            | MagicClass
            | MagicFunction
            | MagicMethod
            | MagicNamespace
            | MagicTrait
            | MagicProperty
    )
}
