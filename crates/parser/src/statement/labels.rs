//! Goto targets are unqualified identifiers, as in `goto retry;`.

use phera_ast::StatementKind;
use phera_lexer::TokenKind;

use crate::{error::ParseError, parser::Parser};

impl Parser<'_> {
    pub(super) fn parse_goto(&mut self) -> Result<StatementKind, ParseError> {
        self.bump()?;
        if !self.at_strict_identifier() {
            let unexpected = self.describe_goto_token();
            return Err(self.error(format!(
                "syntax error, unexpected {unexpected}, expecting identifier"
            )));
        }
        let name = self.parse_strict_identifier()?;
        if !self.eat(TokenKind::Semicolon)? && !self.eat(TokenKind::CloseTag)? {
            let unexpected = self.describe_goto_token();
            return Err(self.error(format!(
                "syntax error, unexpected {unexpected}, expecting \";\""
            )));
        }
        Ok(StatementKind::Goto(name))
    }

    fn describe_goto_token(&self) -> String {
        let kind = match self.kind() {
            TokenKind::Identifier => "identifier",
            TokenKind::QualifiedName => "namespaced name",
            TokenKind::FullyQualifiedName => "fully qualified name",
            TokenKind::RelativeName => "namespace-relative name",
            TokenKind::Variable => "variable",
            TokenKind::Integer => "integer",
            TokenKind::Float => "floating-point number",
            TokenKind::String => {
                return format!(
                    "double-quoted string \"{}\"",
                    self.text().trim_matches(['\'', '"'])
                );
            }
            TokenKind::MagicDir
            | TokenKind::MagicFile
            | TokenKind::MagicLine
            | TokenKind::MagicClass
            | TokenKind::MagicFunction
            | TokenKind::MagicMethod
            | TokenKind::MagicNamespace
            | TokenKind::MagicTrait
            | TokenKind::MagicProperty => {
                return format!("token \"{}\"", self.text().to_ascii_uppercase());
            }
            TokenKind::Exit => return "token \"exit\"".into(),
            kind if crate::parser::is_word_token(kind) => {
                return format!("token \"{}\"", self.text().to_ascii_lowercase());
            }
            _ => return self.describe_current(),
        };
        let token = self.describe_current();
        format!("{kind} {}", token.trim_start_matches("token "))
    }
}
