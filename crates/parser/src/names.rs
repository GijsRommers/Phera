//! Names, identifiers and types.
//!
//! PHP allows reserved words in many name positions (`$a->list`,
//! `Foo::class`, `function list()` as a method, `foo(array: 1)`), so the
//! parser accepts any keyword token where the grammar's `identifier` or
//! `semi_reserved` nonterminals appear and keeps the source text as the name.

use phera_ast::{Identifier, Name, NameKind, Type, TypeKind};
use phera_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::{Parser, is_word_token};

impl Parser<'_> {
    /// Parses a `name`: `Foo`, `Foo\Bar`, `\Foo\Bar` or `namespace\Foo`.
    /// The returned text has no leading `\` and no `namespace\` prefix.
    pub(crate) fn parse_name(&mut self) -> Result<Name, ParseError> {
        let token = self.peek_span();
        let text = self.text();
        let text = text.as_ref();

        let (kind, text) = match self.kind() {
            TokenKind::Identifier => (NameKind::Unqualified, text),
            TokenKind::QualifiedName => (NameKind::Qualified, text),
            TokenKind::FullyQualifiedName => (NameKind::FullyQualified, &text[1..]),
            TokenKind::RelativeName => {
                let separator = text.find('\\').unwrap_or(0);
                (NameKind::Relative, &text[separator + 1..])
            }
            _ => {
                return Err(self.error(format!(
                    "expected a name, found {}",
                    self.describe_current()
                )));
            }
        };

        self.bump()?;
        Ok(Name {
            kind,
            text: text.to_string(),
            span: token,
        })
    }

    /// Parses an identifier where PHP allows semi-reserved words: any keyword
    /// token or plain identifier, with the source text as the name.
    pub(crate) fn parse_identifier(&mut self) -> Result<Identifier, ParseError> {
        let span = self.peek_span();

        if self.at(TokenKind::OpenTagWithEcho) {
            return Err(self.error("Cannot use \"<?=\" as an identifier"));
        }
        if !is_word_token(self.kind()) {
            return Err(self.error(format!(
                "expected an identifier, found {}",
                self.describe_current()
            )));
        }

        let name = self.text().to_string();
        self.bump()?;
        Ok(Identifier { name, span })
    }

    /// Parses an identifier where the grammar allows only `T_STRING` (labels,
    /// aliases, top-level constant names, ...).
    pub(crate) fn parse_strict_identifier(&mut self) -> Result<Identifier, ParseError> {
        let span = self.peek_span();

        // `true`, `false` and `null` are plain `T_STRING`s in PHP's lexer, so
        // they are valid wherever a strict identifier is expected.
        if self.kind() != TokenKind::Identifier
            && !matches!(
                self.kind(),
                TokenKind::True | TokenKind::False | TokenKind::Null
            )
        {
            return Err(self.error(format!(
                "expected an identifier, found {}",
                self.describe_current()
            )));
        }

        let name = self.text().to_string();
        self.bump()?;
        Ok(Identifier { name, span })
    }

    /// Parses a class name: `static` or a plain name. Keywords other than
    /// `static` are not accepted here, matching PHP.
    pub(crate) fn parse_class_name(&mut self) -> Result<Name, ParseError> {
        if self.at(TokenKind::Static) {
            let span = self.peek_span();
            self.bump()?;
            return Ok(Name {
                kind: NameKind::Unqualified,
                text: "static".to_string(),
                span,
            });
        }

        self.parse_name()
    }

    /// Whether the current token can start a type. `allow_static` is false in
    /// parameter and property position, where the grammar forbids `static`.
    pub(crate) fn can_start_type(&self, allow_static: bool) -> bool {
        matches!(
            self.kind(),
            TokenKind::Identifier
                | TokenKind::QualifiedName
                | TokenKind::FullyQualifiedName
                | TokenKind::RelativeName
                | TokenKind::Array
                | TokenKind::Callable
                | TokenKind::Question
                | TokenKind::LeftParen
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
        ) || (allow_static && self.at(TokenKind::Static))
    }

    /// Parses `type_expr`: `T`, `?T`, `A|B`, `A&B`, `(A&B)|C` (DNF).
    pub(crate) fn parse_type_expr(&mut self, allow_static: bool) -> Result<Type, ParseError> {
        let start = self.peek_span();

        if self.eat(TokenKind::Question)? {
            let inner = self.parse_type(allow_static)?;
            return Ok(Type {
                kind: TypeKind::Nullable(Box::new(inner)),
                span: self.span_since(start),
            });
        }

        let first = self.parse_type_element(allow_static)?;

        if self.at(TokenKind::Pipe) {
            let mut types = vec![first];
            while self.eat(TokenKind::Pipe)? {
                types.push(self.parse_type_element(allow_static)?);
            }
            return Ok(Type {
                kind: TypeKind::Union(types),
                span: self.span_since(start),
            });
        }

        // An `&` directly before a variable or `...` is a by-reference
        // parameter marker, not an intersection (PHP's scanner splits the two
        // `&` tokens; Phera's lexer has one).
        if self.at(TokenKind::Ampersand)
            && !matches!(self.kind2(), TokenKind::Variable | TokenKind::Ellipsis)
        {
            let mut types = vec![first];
            while self.eat(TokenKind::Ampersand)? {
                if matches!(self.kind(), TokenKind::Variable | TokenKind::Ellipsis) {
                    break;
                }
                types.push(self.parse_type(allow_static)?);
            }
            if types.len() > 1 {
                return Ok(Type {
                    kind: TypeKind::Intersection(types),
                    span: self.span_since(start),
                });
            }
            return Ok(types.pop().unwrap());
        }

        Ok(first)
    }

    /// One element of a union type: a plain type, or `(A&B)` for DNF types.
    fn parse_type_element(&mut self, allow_static: bool) -> Result<Type, ParseError> {
        if self.at(TokenKind::LeftParen) {
            self.bump()?;
            let start = self.peek_span();
            let first = self.parse_type(allow_static)?;
            let mut types = vec![first];
            while self.at(TokenKind::Ampersand)
                && !matches!(self.kind2(), TokenKind::Variable | TokenKind::Ellipsis)
            {
                self.bump()?;
                types.push(self.parse_type(allow_static)?);
            }
            self.expect(TokenKind::RightParen, "')'")?;
            return Ok(Type {
                kind: TypeKind::Intersection(types),
                span: self.span_since(start),
            });
        }

        self.parse_type(allow_static)
    }

    /// A single non-union, non-nullable type.
    fn parse_type(&mut self, allow_static: bool) -> Result<Type, ParseError> {
        let start = self.peek_span();

        let name = match self.kind() {
            TokenKind::Array => {
                self.bump()?;
                Name {
                    kind: NameKind::Unqualified,
                    text: "array".to_string(),
                    span: start,
                }
            }
            TokenKind::Callable => {
                self.bump()?;
                Name {
                    kind: NameKind::Unqualified,
                    text: "callable".to_string(),
                    span: start,
                }
            }
            // `null`, `true` and `false` are standalone types since PHP 8.2;
            // the lexer lexes them as keyword tokens.
            TokenKind::Null | TokenKind::True | TokenKind::False => {
                let text = self.text().to_ascii_lowercase();
                self.bump()?;
                Name {
                    kind: NameKind::Unqualified,
                    text,
                    span: start,
                }
            }
            TokenKind::Static if allow_static => {
                self.bump()?;
                Name {
                    kind: NameKind::Unqualified,
                    text: "static".to_string(),
                    span: start,
                }
            }
            TokenKind::Identifier
            | TokenKind::QualifiedName
            | TokenKind::FullyQualifiedName
            | TokenKind::RelativeName => self.parse_name()?,
            _ => {
                return Err(self.error(format!(
                    "expected a type, found {}",
                    self.describe_current()
                )));
            }
        };

        Ok(Type {
            kind: TypeKind::Named(name),
            span: self.span_since(start),
        })
    }
}
