//! Parse `(Readable&Seekable)|null` with PHPDoc's intersection precedence.
//! Each nested type has a depth limit. A malformed type returns a diagnostic offset.

use super::token::{self, Lexeme, Token};
use crate::{names::Names, types::*};
use std::sync::Arc;

mod compound;
mod primitive;

const MAX_NESTING: usize = 48;

pub struct Parsed<'a> {
    pub ty: ValueType,
    pub suffix: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    pub offset: usize,
    pub expected: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Templates {
    pub(super) parameters: std::collections::BTreeMap<String, ValueType>,
    pub(super) aliases: std::collections::BTreeMap<String, ValueType>,
    pub(super) imported_aliases: std::collections::BTreeSet<String>,
}
type Result<T> = std::result::Result<T, ParseError>;

pub fn parse<'a>(source: &'a str, names: &Names, templates: &Templates) -> Result<Parsed<'a>> {
    let mut parser = Parser {
        source,
        tokens: token::lex(source),
        cursor: 0,
        names,
        templates,
    };
    let ty = parser.ty(0)?;
    Ok(Parsed {
        ty,
        suffix: &source[parser.offset()..],
    })
}

struct Parser<'a, 'ctx> {
    source: &'a str,
    tokens: Vec<Lexeme<'a>>,
    cursor: usize,
    names: &'ctx Names,
    templates: &'ctx Templates,
}

impl Parser<'_, '_> {
    fn ty(&mut self, depth: usize) -> Result<ValueType> {
        if depth >= MAX_NESTING {
            return Err(self.error("a less deeply nested type"));
        }
        let mut ty = self.intersection(depth)?;
        while self.eat('|') {
            ty = ty.join(&self.intersection(depth)?);
        }
        Ok(ty)
    }

    fn intersection(&mut self, depth: usize) -> Result<ValueType> {
        let mut ty = self.primary(depth)?;
        while self.peek() == Some(Token::Punctuation('&'))
            && !matches!(self.peek_at(1), Some(Token::Variable(_) | Token::Dots))
        {
            self.cursor += 1;
            ty = ValueType::intersection(&ty, &self.primary(depth)?);
        }
        Ok(ty)
    }

    fn primary(&mut self, depth: usize) -> Result<ValueType> {
        if depth >= MAX_NESTING {
            return Err(self.error("a less deeply nested type"));
        }
        let offset = self.offset();
        let mut ty = match self.next() {
            Some(Token::Punctuation('?')) => self.primary(depth + 1)?.join(&ValueType::Null),
            Some(Token::Punctuation('(')) => self.parenthesized(depth + 1)?,
            Some(Token::Variable("this")) => ValueType::This,
            Some(Token::Quoted(value)) => ValueType::StringLiteral(value.as_bytes().into()),
            Some(Token::Word(word)) => self.named_type(word, depth + 1)?,
            _ => {
                return Err(ParseError {
                    offset,
                    expected: "a type",
                });
            }
        };
        while self.peek() == Some(Token::Punctuation('[')) {
            self.cursor += 1;
            if self.eat(']') {
                ty = primitive::collection(CollectionKind::Array, false, vec![ty]);
            } else {
                // Offset access has a known grammar even when the container
                // cannot be evaluated statically, e.g. T['field'].
                self.ty(depth + 1)?;
                self.need(']', "']'")?;
                ty = ValueType::Mixed;
            }
        }
        Ok(ty)
    }

    fn named_type(&mut self, word: &str, depth: usize) -> Result<ValueType> {
        if let Ok(value) = word.parse::<i64>() {
            return Ok(ValueType::IntLiteral(value));
        }
        if self.peek() == Some(Token::DoubleColon) {
            self.cursor += 1;
            let name = match self.next() {
                Some(Token::Word(name)) => name,
                Some(Token::Punctuation('*')) => "*",
                _ => return Err(self.error("a class constant name")),
            };
            let mut name = name.to_string();
            if self.eat('*') {
                name.push('*');
            }
            return Ok(ValueType::TypeConstant(Arc::new(Constant {
                class: super::class_name(word, self.names).unwrap_or_else(|| word.into()),
                name,
            })));
        }
        if self.eat('*') {
            // Global constant patterns, such as FILTER_FLAG_*, may include
            // constants supplied by an optional extension.
            return Ok(ValueType::Mixed);
        }
        let lower = word.to_ascii_lowercase();
        if matches!(lower.as_str(), "array" | "list" | "object") && self.eat('{') {
            return self.shape(&lower, depth);
        }
        if matches!(
            lower.as_str(),
            "callable" | "closure" | "\\closure" | "pure-callable" | "pure-closure"
        ) && self.eat('(')
        {
            return self.callable(
                depth,
                matches!(lower.as_str(), "closure" | "\\closure" | "pure-closure"),
            );
        }
        let arguments = if self.eat('<') {
            self.type_list(depth, '>')?
        } else {
            Vec::new()
        };
        Ok(primitive::from_name(
            word,
            &lower,
            arguments,
            self.names,
            self.templates,
        ))
    }

    fn type_list(&mut self, depth: usize, end: char) -> Result<Vec<ValueType>> {
        if self.eat(end) {
            return Err(self.error("a type argument"));
        }
        let mut values = Vec::new();
        loop {
            if matches!(
                self.peek(),
                Some(Token::Word("covariant" | "contravariant"))
            ) {
                self.cursor += 1;
            }
            if self.eat('*') {
                values.push(ValueType::Mixed);
            } else {
                values.push(self.ty(depth)?);
            }
            if self.eat(end) {
                return Ok(values);
            }
            self.need(',', "',' between type arguments")?;
        }
    }

    fn parenthesized(&mut self, depth: usize) -> Result<ValueType> {
        let variable = match (self.peek(), self.peek_at(1)) {
            (Some(Token::Variable(name)), Some(Token::Word("is"))) if name != "this" => {
                let name = name.to_string();
                self.cursor += 1;
                Some(name)
            }
            _ => None,
        };
        let subject = if variable.is_some() {
            ValueType::Mixed
        } else {
            self.ty(depth)?
        };
        if self.peek() == Some(Token::Word("is")) {
            self.cursor += 1;
            let test = if self.peek() == Some(Token::Word("not")) {
                self.cursor += 1;
                ConditionTest::IsNot
            } else {
                ConditionTest::Is
            };
            let tested = self.ty(depth)?;
            self.need('?', "'?'")?;
            let yes = self.ty(depth)?;
            self.need(':', "':'")?;
            let no = self.ty(depth)?;
            self.need(')', "')'")?;
            return Ok(ValueType::Conditional(Arc::new(Conditional {
                variable,
                subject,
                tested,
                test,
                yes,
                no,
            })));
        }
        self.need(')', "')'")?;
        Ok(subject)
    }

    fn offset(&self) -> usize {
        self.tokens
            .get(self.cursor)
            .map_or(self.source.len(), |lexeme| lexeme.offset)
    }

    fn error(&self, expected: &'static str) -> ParseError {
        ParseError {
            offset: self.offset(),
            expected,
        }
    }

    fn peek_at(&self, ahead: usize) -> Option<Token<'_>> {
        self.tokens
            .get(self.cursor + ahead)
            .map(|lexeme| lexeme.token)
    }

    fn peek(&self) -> Option<Token<'_>> {
        self.peek_at(0)
    }

    fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(Token::Punctuation(ch)) {
            self.cursor += 1;
            true
        } else {
            false
        }
    }

    fn need(&mut self, ch: char, expected: &'static str) -> Result<()> {
        if self.eat(ch) {
            Ok(())
        } else {
            Err(self.error(expected))
        }
    }
}

impl<'a> Parser<'a, '_> {
    fn next(&mut self) -> Option<Token<'a>> {
        let token = self.tokens.get(self.cursor)?.token;
        self.cursor += 1;
        Some(token)
    }
}
