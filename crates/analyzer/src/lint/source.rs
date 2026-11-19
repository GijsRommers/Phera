//! Lossless source tokens, including the comment in `use /* reason */ App\\Name;`.

use phera_lexer::{Lexer, Token, TokenKind};
use phera_source::Span;
use std::cell::OnceCell;

pub(super) struct Source<'a> {
    pub bytes: &'a [u8],
    tokens: OnceCell<Vec<Token>>,
}

impl<'a> Source<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            tokens: OnceCell::new(),
        }
    }

    pub(super) fn slice(&self, span: Span) -> &'a [u8] {
        self.bytes.get(span.start..span.end).unwrap_or_default()
    }

    pub(super) fn tokens(&self) -> &[Token] {
        self.tokens.get_or_init(|| {
            let mut lexer = Lexer::for_tokenizer(self.bytes);
            std::iter::from_fn(|| {
                let token = lexer.next_token().ok()?;
                (token.kind != TokenKind::Eof).then_some(token)
            })
            .collect()
        })
    }

    pub(super) fn within(&self, span: Span) -> &[Token] {
        let tokens = self.tokens();
        let start = tokens.partition_point(|token| token.span.end <= span.start);
        let end = tokens.partition_point(|token| token.span.start < span.end);
        tokens.get(start..end).unwrap_or_default()
    }
}

pub(super) fn is_comment(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Comment | TokenKind::DocComment)
}
