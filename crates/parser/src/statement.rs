//! Statements, including the alternative (`if: ... endif;`) syntax, which
//! produces the same nodes as the brace forms.

use phera_ast::{
    Block, Catch, DeclareDirective, ElseIf, Expression, ExpressionKind, Literal, Statement,
    StatementKind, StaticVariable, SwitchCase,
};
use phera_lexer::TokenKind;

use crate::error::ParseError;
use crate::parser::Parser;

mod labels;

impl Parser<'_> {
    /// A top-level statement: an attributed declaration, a `use`, or a plain
    /// statement.
    pub(crate) fn parse_top_statement(&mut self) -> Result<Statement, ParseError> {
        if self.at(TokenKind::AttributeStart) {
            let start = self.peek_span();
            let attributes = self.parse_attributes()?;
            if matches!(self.kind(), TokenKind::Fn)
                || (self.at(TokenKind::Function) && self.at2(TokenKind::LeftParen))
                || (self.at(TokenKind::Function)
                    && self.at2(TokenKind::Ampersand)
                    && self.kind3() == TokenKind::LeftParen)
                || (self.at(TokenKind::Static)
                    && matches!(self.kind2(), TokenKind::Function | TokenKind::Fn)
                    && (self.kind3() == TokenKind::LeftParen
                        || (self.kind3() == TokenKind::Ampersand
                            && self.kind4() == TokenKind::LeftParen)))
            {
                // `#[A] fn() => 1;` at the top level is an expression statement.
                let is_static = self.eat(TokenKind::Static)?;
                let expression = self.parse_inline_function(attributes, is_static, start)?;
                self.expect_end()?;
                return Ok(Statement {
                    kind: StatementKind::Expression(expression),
                    span: self.span_since(start),
                });
            }
            let mut statement = self.parse_attributed_declaration(attributes, true)?;
            statement.span = self.span_since(start);
            return Ok(statement);
        }

        if self.at(TokenKind::Use) && !self.at2(TokenKind::Colon) {
            return self.parse_use_statement();
        }

        self.parse_statement(true)
    }

    /// Parses one statement. `top` allows `__halt_compiler()` (and top-level
    /// `const`, handled by the caller).
    pub(crate) fn parse_statement(&mut self, top: bool) -> Result<Statement, ParseError> {
        self.enter()?;
        let result = self.parse_statement_inner(top);
        self.leave();
        result
    }

    fn parse_statement_inner(&mut self, top: bool) -> Result<Statement, ParseError> {
        let start = self.peek_span();

        if self.at2(TokenKind::Colon) && crate::parser::is_word_token(self.kind()) {
            let kind = self.parse_label(top)?;
            return Ok(Statement {
                kind,
                span: self.span_since(start),
            });
        }

        let kind = match self.kind() {
            TokenKind::LeftBrace => {
                self.bump()?;
                // On PHP's stack: the `{` and the statement list.
                let statements = self.with_php_slots(1, Self::parse_inner_statements)?;
                self.close_brace()?;
                StatementKind::Block(statements)
            }
            TokenKind::If => return self.parse_if(),
            TokenKind::While => return self.parse_while(),
            TokenKind::Do => return self.parse_do_while(),
            TokenKind::For => return self.parse_for(),
            TokenKind::Foreach => return self.parse_foreach(),
            TokenKind::Switch => return self.parse_switch(),
            TokenKind::Try => return self.parse_try(),
            TokenKind::Declare => return self.parse_declare(),
            TokenKind::Break => {
                self.bump()?;
                let level = self.parse_jump_level("break")?;
                self.expect_end()?;
                StatementKind::Break(level)
            }
            TokenKind::Continue => {
                self.bump()?;
                let level = self.parse_jump_level("continue")?;
                self.expect_end()?;
                StatementKind::Continue(level)
            }
            TokenKind::Return => {
                self.bump()?;
                let value = if self.at(TokenKind::Semicolon) || self.at(TokenKind::CloseTag) {
                    None
                } else {
                    Some(self.parse_expr(0)?)
                };
                self.expect_end()?;
                StatementKind::Return(value)
            }
            TokenKind::Echo | TokenKind::OpenTagWithEcho => return self.parse_echo(),
            TokenKind::Global => {
                self.bump()?;
                let mut variables = Vec::new();
                loop {
                    let expression = self.parse_simple_variable()?;
                    let ExpressionKind::Variable(variable) = expression.into_kind() else {
                        return Err(self.error("expected a variable in 'global'"));
                    };
                    variables.push(variable);
                    if !self.eat(TokenKind::Comma)? {
                        break;
                    }
                }
                self.expect_end()?;
                StatementKind::Global(variables)
            }
            TokenKind::Static
                if !matches!(
                    self.kind2(),
                    TokenKind::Function | TokenKind::Fn | TokenKind::DoubleColon
                ) =>
            {
                self.bump()?;
                let mut variables = Vec::new();
                loop {
                    let start = self.peek_span();
                    let token = self.expect(TokenKind::Variable, "a variable")?;
                    let name = self.text_of(&token)[1..].to_string();
                    let default = if self.eat(TokenKind::Assign)? {
                        Some(self.parse_expr(0)?)
                    } else {
                        None
                    };
                    variables.push(StaticVariable {
                        name,
                        default,
                        span: self.span_since(start),
                    });
                    if !self.eat(TokenKind::Comma)? {
                        break;
                    }
                }
                self.expect_end()?;
                StatementKind::Static(variables)
            }
            TokenKind::Unset => {
                self.bump()?;
                self.expect(TokenKind::LeftParen, "'('")?;
                let mut variables = Vec::new();
                if !self.at(TokenKind::RightParen) {
                    loop {
                        variables.push(self.parse_expr(0)?);
                        if !self.eat(TokenKind::Comma)? {
                            break;
                        }
                        if self.at(TokenKind::RightParen) {
                            break;
                        }
                    }
                }
                self.expect(TokenKind::RightParen, "')'")?;
                self.expect_end()?;
                StatementKind::Unset(variables)
            }
            TokenKind::Goto => self.parse_goto()?,
            TokenKind::AttributeStart => {
                let attributes = self.parse_attributes()?;
                if matches!(self.kind(), TokenKind::Fn)
                    || (self.at(TokenKind::Function) && self.at2(TokenKind::LeftParen))
                    || (self.at(TokenKind::Function)
                        && self.at2(TokenKind::Ampersand)
                        && self.kind3() == TokenKind::LeftParen)
                    || (self.at(TokenKind::Static)
                        && matches!(self.kind2(), TokenKind::Function | TokenKind::Fn)
                        && (self.kind3() == TokenKind::LeftParen
                            || (self.kind3() == TokenKind::Ampersand
                                && self.kind4() == TokenKind::LeftParen)))
                {
                    // `#[A] function () {}` as a statement is a closure expression.
                    let is_static = self.eat(TokenKind::Static)?;
                    let expression = self.parse_inline_function(attributes, is_static, start)?;
                    self.expect_end()?;
                    StatementKind::Expression(expression)
                } else {
                    let mut statement = self.parse_attributed_declaration(attributes, false)?;
                    statement.span = self.span_since(start);
                    return Ok(statement);
                }
            }
            TokenKind::HaltCompiler => return self.parse_halt_compiler(),
            TokenKind::Function
                if matches!(self.kind2(), TokenKind::LeftParen)
                    || (self.at2(TokenKind::Ampersand) && self.kind3() == TokenKind::LeftParen) =>
            {
                // `function () {}` / `function &() {}` as a statement is a
                // closure expression, not a declaration.
                let expression = self.parse_expr(0)?;
                self.expect_end()?;
                StatementKind::Expression(expression)
            }
            // `readonly(...)` is a call to a function named `readonly`.
            TokenKind::Readonly if self.at2(TokenKind::LeftParen) => {
                let expression = self.parse_expr(0)?;
                self.expect_end()?;
                StatementKind::Expression(expression)
            }
            TokenKind::Function
            | TokenKind::Class
            | TokenKind::Trait
            | TokenKind::Interface
            | TokenKind::Enum
            | TokenKind::Abstract
            | TokenKind::Final
            | TokenKind::Readonly => {
                return self.parse_attributed_declaration(Vec::new(), top);
            }
            TokenKind::Const if top => return self.parse_const_statement(Vec::new()),
            _ => {
                let expression = self.parse_expr(0)?;
                self.expect_end()?;
                StatementKind::Expression(expression)
            }
        };

        Ok(Statement {
            kind,
            span: self.span_since(start),
        })
    }

    /// Statements until `}` or end of file.
    pub(crate) fn parse_inner_statements(&mut self) -> Result<Block, ParseError> {
        let mut statements = Vec::new();

        while self.kind() != TokenKind::RightBrace && self.kind() != TokenKind::Eof && !self.halted
        {
            if let Some(statement) = self.parse_list_entry()? {
                statements.push(statement);
            }
        }

        Ok(statements)
    }

    /// One entry of a statement list: `;` and close tags are empty statements
    /// (PHP's parser mode reads `?>` as `;`), tags are skipped, and inline
    /// HTML is a statement of its own. Returns `None` for the empty ones.
    fn parse_list_entry(&mut self) -> Result<Option<Statement>, ParseError> {
        match self.kind() {
            TokenKind::Semicolon | TokenKind::CloseTag | TokenKind::OpenTag => {
                self.bump()?;
                Ok(None)
            }
            TokenKind::InlineHtml if !self.halted => {
                let token = self.bump()?;
                Ok(Some(Statement {
                    kind: StatementKind::InlineHtml(self.bytes_of(&token).to_vec()),
                    span: token.span,
                }))
            }
            TokenKind::InlineHtml => {
                // Raw data after `__halt_compiler();`.
                self.bump()?;
                Ok(None)
            }
            _ => Ok(Some(self.parse_statement(false)?)),
        }
    }

    /// A loop/if body: `{ ... }` gives its statements directly, a single
    /// statement becomes a one-element block.
    fn parse_body(&mut self) -> Result<Block, ParseError> {
        if self.at(TokenKind::Semicolon) {
            // `if ($a);` has an empty body.
            self.bump()?;
            return Ok(Vec::new());
        }
        if self.at(TokenKind::LeftBrace) {
            self.bump()?;
            let statements = self.with_php_slots(1, Self::parse_inner_statements)?;
            self.close_brace()?;
            Ok(statements)
        } else {
            Ok(vec![self.parse_statement(false)?])
        }
    }

    /// Statements of an alternative-syntax body, up to one of `stoppers`.
    fn parse_alt_statements(&mut self, stoppers: &[TokenKind]) -> Result<Block, ParseError> {
        let mut statements = Vec::new();

        while !stoppers.contains(&self.kind())
            && self.kind() != TokenKind::Eof
            && self.kind() != TokenKind::RightBrace
            && !self.halted
        {
            if let Some(statement) = self.parse_list_entry()? {
                statements.push(statement);
            }
        }

        Ok(statements)
    }

    /// `if` / `elseif` / `else`, in both brace and alternative syntax. Each
    /// `elseif` nests as a single `If` in the previous `otherwise`.
    fn parse_if(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::If, "'if'")?.span;
        let condition = self.parse_paren_expr()?;
        let alt = self.at(TokenKind::Colon);
        // On PHP's stack: `if`, `(`, the condition and `)`.
        let then = self.with_php_slots(4, |parser| {
            if alt {
                parser.bump()?;
                parser.parse_alt_statements(&[TokenKind::ElseIf, TokenKind::Else, TokenKind::EndIf])
            } else {
                parser.parse_body()
            }
        })?;
        let (elseifs, otherwise) = self.parse_if_tail(alt)?;

        Ok(Statement {
            kind: StatementKind::If {
                condition,
                then,
                elseifs,
                otherwise,
            },
            span: self.span_since(start),
        })
    }

    /// The `elseif`/`else`/`endif` part after an if body. The `elseif`
    /// branches are collected in a loop: a ladder is as long as it likes.
    fn parse_if_tail(&mut self, alt: bool) -> Result<(Vec<ElseIf>, Option<Block>), ParseError> {
        let mut elseifs = Vec::new();
        while self.at(TokenKind::ElseIf) {
            let start = self.peek_span();
            self.bump()?;
            let condition = self.parse_paren_expr()?;
            let body = self.with_php_slots(4, |parser| {
                if alt {
                    parser.expect(TokenKind::Colon, "':'")?;
                    parser.parse_alt_statements(&[
                        TokenKind::ElseIf,
                        TokenKind::Else,
                        TokenKind::EndIf,
                    ])
                } else {
                    parser.parse_body()
                }
            })?;
            elseifs.push(ElseIf {
                condition,
                body,
                span: self.span_since(start),
            });
        }

        let otherwise = if self.at(TokenKind::Else) {
            self.bump()?;
            if alt {
                self.expect(TokenKind::Colon, "':'")?;
                let body = self.parse_alt_statements(&[TokenKind::EndIf])?;
                self.expect(TokenKind::EndIf, "'endif'")?;
                self.expect_end()?;
                Some(body)
            } else {
                // On PHP's stack: the `if` so far and the `else`.
                Some(self.with_php_slots(1, Self::parse_body)?)
            }
        } else {
            if alt {
                self.expect(TokenKind::EndIf, "'endif'")?;
                self.expect_end()?;
            }
            None
        };
        Ok((elseifs, otherwise))
    }

    fn parse_while(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::While, "'while'")?.span;
        let condition = self.parse_paren_expr()?;
        let body = if self.at(TokenKind::Colon) {
            self.bump()?;
            let body = self.parse_alt_statements(&[TokenKind::EndWhile])?;
            self.expect(TokenKind::EndWhile, "'endwhile'")?;
            self.expect_end()?;
            body
        } else {
            // On PHP's stack: `while`, `(`, the condition and `)`.
            self.with_php_slots(4, Self::parse_body)?
        };

        Ok(Statement {
            kind: StatementKind::While { condition, body },
            span: self.span_since(start),
        })
    }

    fn parse_do_while(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Do, "'do'")?.span;
        let body = self.with_php_slots(1, Self::parse_body)?;
        self.expect(TokenKind::While, "'while'")?;
        let condition = self.parse_paren_expr()?;
        self.expect_end()?;

        Ok(Statement {
            kind: StatementKind::DoWhile { body, condition },
            span: self.span_since(start),
        })
    }

    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::For, "'for'")?.span;
        self.expect(TokenKind::LeftParen, "'('")?;
        let init = self.parse_expr_list(TokenKind::Semicolon)?;
        self.expect(TokenKind::Semicolon, "';'")?;
        let condition = self.parse_expr_list(TokenKind::Semicolon)?;
        self.expect(TokenKind::Semicolon, "';'")?;
        let step = self.parse_expr_list(TokenKind::RightParen)?;
        self.expect(TokenKind::RightParen, "')'")?;

        let body = if self.at(TokenKind::Colon) {
            self.bump()?;
            let body = self.parse_alt_statements(&[TokenKind::EndFor])?;
            self.expect(TokenKind::EndFor, "'endfor'")?;
            self.expect_end()?;
            body
        } else {
            // On PHP's stack: `for`, `(`, three expression lists, two `;`, `)`.
            self.with_php_slots(8, Self::parse_body)?
        };

        Ok(Statement {
            kind: StatementKind::For {
                init,
                condition,
                step,
                body,
            },
            span: self.span_since(start),
        })
    }

    fn parse_foreach(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Foreach, "'foreach'")?.span;
        self.expect(TokenKind::LeftParen, "'('")?;
        let subject = self.parse_expr(0)?;
        self.expect(TokenKind::As, "'as'")?;

        // `foreach ($x as $k => $v)`: the first parsed variable is the key.
        let (first, first_by_ref) = self.parse_foreach_variable()?;
        let (key, value, by_reference) = if self.eat(TokenKind::FatArrow)? {
            let (value, by_reference) = self.parse_foreach_variable()?;
            (Some(first), value, by_reference)
        } else {
            (None, first, first_by_ref)
        };

        self.expect(TokenKind::RightParen, "')'")?;

        let body = if self.at(TokenKind::Colon) {
            self.bump()?;
            let body = self.parse_alt_statements(&[TokenKind::EndForeach])?;
            self.expect(TokenKind::EndForeach, "'endforeach'")?;
            self.expect_end()?;
            body
        } else {
            // On PHP's stack: `foreach`, `(`, the subject, `as`, the variable, `)`.
            self.with_php_slots(6, Self::parse_body)?
        };

        Ok(Statement {
            kind: StatementKind::Foreach {
                subject,
                key,
                value,
                by_reference,
                body,
            },
            span: self.span_since(start),
        })
    }

    /// One `as` variable of a foreach: `[$a, $b]`, `list(...)`, `&$v` or a
    /// plain expression. Returns the variable and whether it is by reference.
    fn parse_foreach_variable(&mut self) -> Result<(Expression, bool), ParseError> {
        if self.at(TokenKind::List) {
            return Ok((self.parse_list_target()?, false));
        }

        if self.at(TokenKind::LeftBracket) {
            // `[...]` here is a destructuring target.
            let start = self.peek_span();
            self.bump()?;
            let pairs = self.parse_array_pairs(TokenKind::RightBracket)?;
            self.expect(TokenKind::RightBracket, "']'")?;
            return Ok((
                Expression {
                    kind: ExpressionKind::List(crate::expression::array_into_list(pairs)),
                    span: self.span_since(start),
                },
                false,
            ));
        }

        if self.at(TokenKind::Ampersand) {
            self.bump()?;
            let expression = self.parse_prefix()?;
            if !crate::expression::is_assignable(&expression.kind) {
                return Err(self.error("expected a variable after '&' in foreach"));
            }
            return Ok((expression, true));
        }

        Ok((self.parse_expr(0)?, false))
    }

    fn parse_switch(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Switch, "'switch'")?.span;
        let subject = self.parse_paren_expr()?;

        let cases = if self.at(TokenKind::Colon) {
            self.bump()?;
            self.eat(TokenKind::Semicolon)?;
            let cases = self.parse_case_list(&[TokenKind::EndSwitch])?;
            self.expect(TokenKind::EndSwitch, "'endswitch'")?;
            self.expect_end()?;
            cases
        } else {
            self.expect(TokenKind::LeftBrace, "'{'")?;
            self.eat(TokenKind::Semicolon)?;
            let cases = self.parse_case_list(&[TokenKind::RightBrace])?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            cases
        };

        Ok(Statement {
            kind: StatementKind::Switch { subject, cases },
            span: self.span_since(start),
        })
    }

    fn parse_case_list(&mut self, stoppers: &[TokenKind]) -> Result<Vec<SwitchCase>, ParseError> {
        let mut cases = Vec::new();

        while !stoppers.contains(&self.kind()) && self.kind() != TokenKind::Eof {
            let start = self.peek_span();
            let condition = if self.eat(TokenKind::Case)? {
                let expression = self.parse_expr(0)?;
                if !self.eat(TokenKind::Colon)? {
                    self.expect(TokenKind::Semicolon, "':' or ';'")?;
                }
                Some(expression)
            } else if self.eat(TokenKind::Default)? {
                if !self.eat(TokenKind::Colon)? {
                    self.expect(TokenKind::Semicolon, "':' or ';'")?;
                }
                None
            } else {
                return Err(self.error(format!(
                    "expected 'case' or 'default', found {}",
                    self.describe_current()
                )));
            };

            let mut body = Vec::new();
            while !stoppers.contains(&self.kind())
                && self.kind() != TokenKind::Case
                && self.kind() != TokenKind::Default
                && self.kind() != TokenKind::Eof
                && self.kind() != TokenKind::RightBrace
                && !self.halted
            {
                if let Some(statement) = self.parse_list_entry()? {
                    body.push(statement);
                }
            }

            cases.push(SwitchCase {
                condition,
                body,
                span: self.span_since(start),
            });
        }

        Ok(cases)
    }

    fn parse_try(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Try, "'try'")?.span;
        self.expect(TokenKind::LeftBrace, "'{'")?;
        let body = self.parse_inner_statements()?;
        self.expect(TokenKind::RightBrace, "'}'")?;

        let mut catches = Vec::new();
        while self.at(TokenKind::Catch) {
            let catch_start = self.peek_span();
            self.bump()?;
            self.expect(TokenKind::LeftParen, "'('")?;
            let mut types = vec![self.parse_class_name()?];
            while self.eat(TokenKind::Pipe)? {
                types.push(self.parse_class_name()?);
            }
            let variable = if self.at(TokenKind::Variable) {
                let token = self.bump()?;
                Some(self.text_of(&token)[1..].to_string())
            } else {
                None
            };
            self.expect(TokenKind::RightParen, "')'")?;
            self.expect(TokenKind::LeftBrace, "'{'")?;
            let catch_body = self.parse_inner_statements()?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            catches.push(Catch {
                types,
                variable,
                body: catch_body,
                span: self.span_since(catch_start),
            });
        }

        let finally = if self.eat(TokenKind::Finally)? {
            self.expect(TokenKind::LeftBrace, "'{'")?;
            let body = self.parse_inner_statements()?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            Some(body)
        } else {
            None
        };

        Ok(Statement {
            kind: StatementKind::Try {
                body,
                catches,
                finally,
            },
            span: self.span_since(start),
        })
    }

    fn parse_declare(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Declare, "'declare'")?.span;
        self.expect(TokenKind::LeftParen, "'('")?;

        let mut directives = Vec::new();
        if !self.at(TokenKind::RightParen) {
            loop {
                let name = self.parse_strict_identifier()?;
                self.expect(TokenKind::Assign, "'='")?;
                let value = self.parse_expr(0)?;
                directives.push(DeclareDirective { name, value });
                if !self.eat(TokenKind::Comma)? {
                    break;
                }
                if self.at(TokenKind::RightParen) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RightParen, "')'")?;

        let body = if self.at(TokenKind::Colon) {
            self.bump()?;
            let body = self.parse_alt_statements(&[TokenKind::EndDeclare])?;
            self.expect(TokenKind::EndDeclare, "'enddeclare'")?;
            self.expect_end()?;
            Some(body)
        } else if self.at(TokenKind::Semicolon) {
            self.bump()?;
            None
        } else {
            Some(self.parse_body()?)
        };

        Ok(Statement {
            kind: StatementKind::Declare { directives, body },
            span: self.span_since(start),
        })
    }

    /// `echo $a, $b;` and `<?= $a ?>`.
    pub(crate) fn parse_echo(&mut self) -> Result<Statement, ParseError> {
        let start = self.peek_span();
        self.bump()?; // `echo` or `<?=`
        let mut expressions = vec![self.parse_expr(0)?];
        while self.eat(TokenKind::Comma)? {
            expressions.push(self.parse_expr(0)?);
        }
        self.expect_end()?;

        Ok(Statement {
            kind: StatementKind::Echo(expressions),
            span: self.span_since(start),
        })
    }

    /// `namespace Foo;`, `namespace Foo { ... }`, `namespace { ... }`. The
    /// unbraced form collects everything up to the next namespace or the end
    /// of the file.
    pub(crate) fn parse_namespace(&mut self) -> Result<Statement, ParseError> {
        let start = self.expect(TokenKind::Namespace, "'namespace'")?.span;

        if self.at(TokenKind::LeftBrace) {
            self.bump()?;
            let statements = self.parse_top_level(&[TokenKind::RightBrace])?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            return Ok(Statement {
                kind: StatementKind::Namespace {
                    name: None,
                    statements,
                    braced: true,
                },
                span: self.span_since(start),
            });
        }

        // `identifier | T_NAME_QUALIFIED`; identifiers may be reserved words.
        let name = if self.at(TokenKind::QualifiedName) {
            Some(self.parse_name()?)
        } else if crate::parser::is_word_token(self.kind()) {
            let identifier = self.parse_identifier()?;
            Some(phera_ast::Name {
                kind: phera_ast::NameKind::Unqualified,
                text: identifier.name,
                span: identifier.span,
            })
        } else {
            return Err(self.error(format!(
                "expected a namespace name, found {}",
                self.describe_current()
            )));
        };

        if self.at(TokenKind::LeftBrace) {
            self.bump()?;
            let statements = self.parse_top_level(&[TokenKind::RightBrace])?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            Ok(Statement {
                kind: StatementKind::Namespace {
                    name,
                    statements,
                    braced: true,
                },
                span: self.span_since(start),
            })
        } else {
            self.expect(TokenKind::Semicolon, "';' or '{'")?;
            let statements = self.parse_top_level(&[TokenKind::Namespace])?;
            Ok(Statement {
                kind: StatementKind::Namespace {
                    name,
                    statements,
                    braced: false,
                },
                span: self.span_since(start),
            })
        }
    }

    /// `__halt_compiler();` — records where the raw data starts and stops
    /// parsing.
    pub(crate) fn parse_halt_compiler(&mut self) -> Result<Statement, ParseError> {
        let start = self
            .expect(TokenKind::HaltCompiler, "'__halt_compiler'")?
            .span;
        self.expect(TokenKind::LeftParen, "'('")?;
        self.expect(TokenKind::RightParen, "')'")?;
        self.expect(TokenKind::Semicolon, "';'")?;

        // The lexer hands the rest of the file back as one InlineHtml token.
        let data_offset = if self.at(TokenKind::InlineHtml) {
            self.peek_span().start
        } else {
            self.prev_end
        };

        self.halted = true;
        Ok(Statement {
            kind: StatementKind::HaltCompiler { data_offset },
            span: self.span_since(start),
        })
    }

    /// `break;` is `Break(1)`; `break 2;` is `Break(2)`.
    fn parse_jump_level(&mut self, keyword: &str) -> Result<u32, ParseError> {
        if self.at(TokenKind::Semicolon) || self.at(TokenKind::CloseTag) {
            return Ok(1);
        }
        // PHP accepts any expression here and rejects invalid levels at
        // compile time; the AST stores a plain level.
        let expression = self.parse_expr(0)?;
        match expression.kind {
            ExpressionKind::Literal(Literal::Int(value)) if value > 0 => {
                Ok(value.min(u32::MAX as i64) as u32)
            }
            ExpressionKind::Literal(Literal::Int(_) | Literal::Float(_) | Literal::String(_)) => {
                Err(ParseError::compile(
                    format!("'{keyword}' operator accepts only positive integers"),
                    expression.span,
                ))
            }
            _ => Err(ParseError::compile(
                format!("'{keyword}' operator with non-integer operand is no longer supported"),
                expression.span,
            )),
        }
    }

    /// A comma-separated expression list up to (not including) `stop`.
    fn parse_expr_list(&mut self, stop: TokenKind) -> Result<Vec<Expression>, ParseError> {
        let mut expressions = Vec::new();

        if !self.at(stop) {
            loop {
                expressions.push(self.parse_expr(0)?);
                if !self.eat(TokenKind::Comma)? {
                    break;
                }
                if self.at(stop) {
                    break;
                }
            }
        }

        Ok(expressions)
    }

    /// Closes a `{ ... }` block; after `__halt_compiler();` every open block
    /// is implicitly closed.
    pub(crate) fn close_brace(&mut self) -> Result<(), ParseError> {
        if self.halted {
            return Ok(());
        }
        self.expect(TokenKind::RightBrace, "'}'")?;
        Ok(())
    }

    /// The end of a statement: `;`, or a close tag, which acts like one.
    pub(crate) fn expect_end(&mut self) -> Result<(), ParseError> {
        if self.eat(TokenKind::Semicolon)? || self.eat(TokenKind::CloseTag)? {
            Ok(())
        } else {
            Err(self.error(format!(
                "expected ';' or close tag, found {}",
                self.describe_current()
            )))
        }
    }
}
