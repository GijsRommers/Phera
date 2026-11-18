//! Goto targets are unqualified identifiers, as in `goto retry;`.

use phera_ast::StatementKind;
use phera_lexer::TokenKind;

use crate::{error::ParseError, parser::Parser};

impl Parser<'_> {
    pub(super) fn parse_label(&mut self, top: bool) -> Result<StatementKind, ParseError> {
        if self.at_strict_identifier() {
            let name = self.parse_strict_identifier()?;
            self.bump()?; // `:`
            return Ok(StatementKind::Label(name));
        }

        // The keyword keeps its ordinary grammar role. Where it begins a
        // statement PHP reports the colon where the next token was needed.
        use TokenKind::*;
        let expectation = match self.kind() {
            And | As | Callable | Case | Catch | Default | Else | ElseIf | EndDeclare | EndFor
            | EndForeach | EndIf | EndSwitch | EndWhile | Extends | Finally | Implements
            | InstanceOf | InsteadOf | Or | Private | Protected | Public | PrivateSet
            | ProtectedSet | PublicSet | Var | Xor => {
                let unexpected = self.describe_goto_token();
                let expectation = if top { ", expecting end of file" } else { "" };
                return Err(self.error(format!(
                    "syntax error, unexpected {unexpected}{expectation}"
                )));
            }
            Abstract | Final | Readonly => {
                ", expecting \"abstract\" or \"final\" or \"readonly\" or \"class\""
            }
            Array | Declare | Empty | Eval | Fn | For | Foreach | Function | HaltCompiler | If
            | Isset | List | Match | Switch | Unset | While => ", expecting \"(\"",
            Break | Continue | Return => ", expecting \";\"",
            Class | Const | Goto | Interface | Trait => ", expecting identifier",
            Global => ", expecting variable or \"$\"",
            Namespace | Try => ", expecting \"{\"",
            New => ", expecting \"class\"",
            Static => ", expecting \"::\"",
            _ => "",
        };
        Err(self.error_at(
            self.second_span(),
            format!("syntax error, unexpected token \":\"{expectation}"),
        ))
    }

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
                let text = self.text();
                let (description, contents) = if let Some(contents) = text.strip_prefix('\'') {
                    (
                        "single-quoted string",
                        contents.strip_suffix('\'').unwrap_or(contents),
                    )
                } else if let Some(contents) = text.strip_prefix('"') {
                    (
                        "double-quoted string",
                        contents.strip_suffix('"').unwrap_or(contents),
                    )
                } else {
                    ("quoted string", text.trim_end_matches(['\'', '"']))
                };
                return format!("{description} \"{contents}\"");
            }
            TokenKind::DoubleQuote => return "double-quote mark".into(),
            TokenKind::YieldFrom => return "token \"yield from\"".into(),
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
