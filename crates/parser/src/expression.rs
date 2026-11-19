//! Expressions: precedence climbing (Pratt) over the token stream.
//!
//! The precedence levels mirror the `%left` / `%right` / `%nonassoc` /
//! `%precedence` block of PHP's `zend_language_parser.y` exactly; higher
//! levels bind tighter. Left-associative chains and postfix operators run in
//! loops, so `1 + 2 + 3 + ...` never recurses per operator.

mod assignment;

use phera_ast::{
    Argument, Arguments, ArrayItem, ArrowFunction, Attribute, BinaryOperator, CastKind, Closure,
    ClosureUse, Expression, ExpressionKind, IncludeKind, Literal, MagicConstant, Match, MatchArm,
    MemberName, Modifiers, NewTarget, UnaryOperator, Variable,
};
use phera_lexer::TokenKind;
use phera_source::Span;

use crate::error::ParseError;
use crate::parser::{Parser, is_word_token};

/// Operator precedence levels, in the order PHP's grammar declares them
/// (later declarations bind tighter).
pub(crate) mod prec {
    pub const THROW: u8 = 1;
    pub const INCLUDE: u8 = 3;
    pub const LOGICAL_OR: u8 = 4; // `or`
    pub const LOGICAL_XOR: u8 = 5; // `xor`
    pub const LOGICAL_AND: u8 = 6; // `and`
    pub const PRINT: u8 = 7;
    pub const YIELD: u8 = 8;
    pub const ASSIGN: u8 = 11; // `=`, `+=`, ... (right)
    pub const TERNARY: u8 = 12; // `? :` (left in the grammar)
    pub const COALESCE: u8 = 13; // `??` (right)
    pub const BOOL_OR: u8 = 14; // `||`
    pub const BOOL_AND: u8 = 15; // `&&`
    pub const BW_OR: u8 = 16; // `|`
    pub const BW_XOR: u8 = 17; // `^`
    pub const BW_AND: u8 = 18; // `&`
    pub const EQUALITY: u8 = 19; // `==` `!=` `===` `!==` `<=>` (nonassoc)
    pub const RELATIONAL: u8 = 20; // `<` `<=` `>` `>=` (nonassoc)
    pub const PIPE: u8 = 21; // `|>`
    pub const CONCAT: u8 = 22; // `.`
    pub const SHIFT: u8 = 23; // `<<` `>>`
    pub const ADD: u8 = 24; // `+` `-`
    pub const MUL: u8 = 25; // `*` `/` `%`
    pub const NOT: u8 = 26; // `!`
    pub const INSTANCEOF: u8 = 27;
    pub const UNARY: u8 = 28; // `~`, casts, `@`, unary `+`/`-`
    pub const POW: u8 = 29; // `**` (right)
    pub const CLONE: u8 = 30;
}

#[derive(Clone, Copy, PartialEq)]
enum Assoc {
    Left,
    Right,
    Nonassoc,
}

/// The binary operators of the precedence table.
fn binary_op(kind: TokenKind) -> Option<(u8, Assoc, BinaryOperator)> {
    use TokenKind::*;
    let (prec, assoc, op) = match kind {
        Or => (prec::LOGICAL_OR, Assoc::Left, BinaryOperator::Or),
        Xor => (prec::LOGICAL_XOR, Assoc::Left, BinaryOperator::Xor),
        And => (prec::LOGICAL_AND, Assoc::Left, BinaryOperator::And),
        LogicalOr => (prec::BOOL_OR, Assoc::Left, BinaryOperator::Or),
        LogicalAnd => (prec::BOOL_AND, Assoc::Left, BinaryOperator::And),
        Pipe => (prec::BW_OR, Assoc::Left, BinaryOperator::BitwiseOr),
        Caret => (prec::BW_XOR, Assoc::Left, BinaryOperator::BitwiseXor),
        Ampersand => (prec::BW_AND, Assoc::Left, BinaryOperator::BitwiseAnd),
        Equal => (prec::EQUALITY, Assoc::Nonassoc, BinaryOperator::Equal),
        NotEqual => (prec::EQUALITY, Assoc::Nonassoc, BinaryOperator::NotEqual),
        Identical => (prec::EQUALITY, Assoc::Nonassoc, BinaryOperator::Identical),
        NotIdentical => (
            prec::EQUALITY,
            Assoc::Nonassoc,
            BinaryOperator::NotIdentical,
        ),
        Spaceship => (prec::EQUALITY, Assoc::Nonassoc, BinaryOperator::Spaceship),
        Less => (prec::RELATIONAL, Assoc::Nonassoc, BinaryOperator::Less),
        LessEqual => (prec::RELATIONAL, Assoc::Nonassoc, BinaryOperator::LessEqual),
        Greater => (prec::RELATIONAL, Assoc::Nonassoc, BinaryOperator::Greater),
        GreaterEqual => (
            prec::RELATIONAL,
            Assoc::Nonassoc,
            BinaryOperator::GreaterEqual,
        ),
        PipeForward => (prec::PIPE, Assoc::Left, BinaryOperator::Pipe),
        Dot => (prec::CONCAT, Assoc::Left, BinaryOperator::Concat),
        ShiftLeft => (prec::SHIFT, Assoc::Left, BinaryOperator::ShiftLeft),
        ShiftRight => (prec::SHIFT, Assoc::Left, BinaryOperator::ShiftRight),
        Plus => (prec::ADD, Assoc::Left, BinaryOperator::Add),
        Minus => (prec::ADD, Assoc::Left, BinaryOperator::Subtract),
        Star => (prec::MUL, Assoc::Left, BinaryOperator::Multiply),
        Slash => (prec::MUL, Assoc::Left, BinaryOperator::Divide),
        Percent => (prec::MUL, Assoc::Left, BinaryOperator::Modulo),
        Power => (prec::POW, Assoc::Right, BinaryOperator::Power),
        NullCoalesce => (prec::COALESCE, Assoc::Right, BinaryOperator::NullCoalesce),
        _ => return None,
    };
    Some((prec, assoc, op))
}

/// Compound assignment operators (`$a += 1`), all at the assignment level.
fn compound_assign_op(kind: TokenKind) -> Option<BinaryOperator> {
    use TokenKind::*;
    let op = match kind {
        PlusAssign => BinaryOperator::Add,
        MinusAssign => BinaryOperator::Subtract,
        StarAssign => BinaryOperator::Multiply,
        SlashAssign => BinaryOperator::Divide,
        PercentAssign => BinaryOperator::Modulo,
        PowerAssign => BinaryOperator::Power,
        DotAssign => BinaryOperator::Concat,
        AmpersandAssign => BinaryOperator::BitwiseAnd,
        PipeAssign => BinaryOperator::BitwiseOr,
        CaretAssign => BinaryOperator::BitwiseXor,
        ShiftLeftAssign => BinaryOperator::ShiftLeft,
        ShiftRightAssign => BinaryOperator::ShiftRight,
        NullCoalesceAssign => BinaryOperator::NullCoalesce,
        _ => return None,
    };
    Some(op)
}

/// Whether an expression is a `variable` in the grammar's sense: the only
/// things that can stand on the left of `=` and friends.
pub(crate) fn is_assignable(kind: &ExpressionKind) -> bool {
    matches!(
        kind,
        ExpressionKind::Variable(_)
            | ExpressionKind::Index { .. }
            | ExpressionKind::Property { .. }
            | ExpressionKind::StaticProperty { .. }
            | ExpressionKind::Call { .. }
            | ExpressionKind::MethodCall { .. }
            | ExpressionKind::StaticCall { .. }
            | ExpressionKind::List(_)
    )
}

impl Parser<'_> {
    /// Parses an expression, consuming only operators with precedence at
    /// least `min_prec`.
    pub(crate) fn parse_expr(&mut self, min_prec: u8) -> Result<Expression, ParseError> {
        self.enter()?;
        let result = self.parse_expr_loop(min_prec);
        self.leave();
        result
    }

    fn parse_expr_loop(&mut self, min_prec: u8) -> Result<Expression, ParseError> {
        let mut left = self.parse_prefix()?;
        // Precedence of the last non-associative operator consumed at this
        // level; a second one at the same level is a syntax error (`1 < 2 < 3`).
        let mut banned: Option<u8> = None;

        loop {
            let kind = self.kind();

            if kind == TokenKind::Assign || compound_assign_op(kind).is_some() {
                if prec::ASSIGN < min_prec {
                    break;
                }
                let assign_token = self.bump()?;
                let compound = compound_assign_op(kind);
                let rhs = if kind == TokenKind::Assign && self.at(TokenKind::Ampersand) {
                    self.bump()?;
                    assignment::Assignment::Reference(self.parse_assign_ref_rhs()?)
                } else {
                    // PHP's stack holds the target and the `=`.
                    let value = self.with_php_slots(1, |parser| parser.parse_expr(prec::ASSIGN))?;
                    assignment::Assignment::Value(value, compound)
                };
                left = assignment::attach(left, rhs, assign_token.span, self)?;
                banned = None;
                continue;
            }

            if kind == TokenKind::Question {
                if prec::TERNARY < min_prec {
                    break;
                }
                left = self.parse_ternary(left)?;
                banned = None;
                continue;
            }

            if kind == TokenKind::InstanceOf {
                if prec::INSTANCEOF < min_prec {
                    break;
                }
                // `a instanceof b instanceof c` chains left-associatively, and
                // the right side is a class name reference, so not even `**`
                // binds into it: `a instanceof b ** c` is `(a instanceof b) ** c`.
                self.bump()?;
                let right = self.parse_expr(prec::CLONE)?;
                let span = self.span_since(left.span);
                left = Expression {
                    kind: ExpressionKind::InstanceOf {
                        value: Box::new(left),
                        class: Box::new(right),
                    },
                    span,
                };
                banned = None;
                continue;
            }

            let Some((level, assoc, op)) = binary_op(kind) else {
                break;
            };
            if level < min_prec {
                break;
            }
            if assoc == Assoc::Nonassoc && banned == Some(level) {
                return Err(self.error(format!(
                    "syntax error: unexpected {}",
                    self.describe_current()
                )));
            }
            self.bump()?;
            let next_min = match assoc {
                Assoc::Left | Assoc::Nonassoc => level + 1,
                Assoc::Right => level,
            };
            // PHP's stack holds the left operand and the operator.
            let right = self.with_php_slots(1, |parser| parser.parse_expr(next_min))?;
            let span = self.span_since(left.span);
            left = Expression {
                kind: ExpressionKind::Binary {
                    operator: op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            };
            banned = if assoc == Assoc::Nonassoc {
                Some(level)
            } else {
                None
            };
        }

        Ok(left)
    }

    /// `cond ? then : otherwise`, or `cond ?: otherwise`. The grammar declares
    /// `?` `:` left-associative, so a following `?` chains onto the whole
    /// ternary: `a ? b : c ? d : e` is `(a ? b : c) ? d : e`.
    fn parse_ternary(&mut self, condition: Expression) -> Result<Expression, ParseError> {
        let start = condition.span;
        self.expect(TokenKind::Question, "'?'")?;
        // The then-branch is a full expression; only the `:` ends it (and a
        // nested ternary may appear inside it).
        let then = if self.at(TokenKind::Colon) {
            None
        } else {
            // PHP's stack holds the condition and the `?`.
            Some(Box::new(
                self.with_php_slots(1, |parser| parser.parse_expr(0))?,
            ))
        };
        self.expect(TokenKind::Colon, "':'")?;
        // ... and then the middle operand and the `:` too.
        let otherwise = self.with_php_slots(3, |parser| parser.parse_expr(prec::TERNARY + 1))?;
        Ok(Expression {
            kind: ExpressionKind::Ternary {
                condition: Box::new(condition),
                then,
                otherwise: Box::new(otherwise),
            },
            span: self.span_since(start),
        })
    }

    /// Prefix operators and everything that can start an expression.
    pub(crate) fn parse_prefix(&mut self) -> Result<Expression, ParseError> {
        let start = self.peek_span();

        match self.kind() {
            TokenKind::Plus => {
                self.bump()?;
                let operand = self.parse_expr(prec::UNARY)?;
                Ok(unary(UnaryOperator::Plus, operand, self.span_since(start)))
            }
            TokenKind::Minus => {
                self.bump()?;
                let operand = self.parse_expr(prec::UNARY)?;
                Ok(unary(UnaryOperator::Minus, operand, self.span_since(start)))
            }
            TokenKind::Not => {
                self.bump()?;
                let operand = self.parse_expr(prec::NOT)?;
                Ok(unary(UnaryOperator::Not, operand, self.span_since(start)))
            }
            TokenKind::Tilde => {
                self.bump()?;
                let operand = self.parse_expr(prec::UNARY)?;
                Ok(unary(
                    UnaryOperator::BitwiseNot,
                    operand,
                    self.span_since(start),
                ))
            }
            TokenKind::At => {
                self.bump()?;
                let operand = self.parse_expr(prec::UNARY)?;
                Ok(unary(
                    UnaryOperator::ErrorSuppress,
                    operand,
                    self.span_since(start),
                ))
            }
            TokenKind::IntCast
            | TokenKind::FloatCast
            | TokenKind::StringCast
            | TokenKind::ArrayCast
            | TokenKind::ObjectCast
            | TokenKind::BoolCast
            | TokenKind::UnsetCast
            | TokenKind::VoidCast => {
                let cast = cast_kind(self.kind());
                self.bump()?;
                let operand = self.parse_expr(prec::UNARY)?;
                Ok(Expression {
                    kind: ExpressionKind::Cast {
                        kind: cast,
                        operand: Box::new(operand),
                    },
                    span: self.span_since(start),
                })
            }
            TokenKind::Include
            | TokenKind::IncludeOnce
            | TokenKind::Require
            | TokenKind::RequireOnce => {
                let kind = match self.kind() {
                    TokenKind::Include => IncludeKind::Include,
                    TokenKind::IncludeOnce => IncludeKind::IncludeOnce,
                    TokenKind::Require => IncludeKind::Require,
                    _ => IncludeKind::RequireOnce,
                };
                self.bump()?;
                let path = self.parse_expr(prec::INCLUDE)?;
                Ok(Expression {
                    kind: ExpressionKind::Include {
                        kind,
                        path: Box::new(path),
                    },
                    span: self.span_since(start),
                })
            }
            TokenKind::Print => {
                self.bump()?;
                let operand = self.parse_expr(prec::PRINT)?;
                Ok(Expression {
                    kind: ExpressionKind::Print(Box::new(operand)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Yield => self.parse_yield(),
            TokenKind::YieldFrom => {
                self.saw_yield = true;
                self.bump()?;
                let operand = self.parse_expr(prec::YIELD + 2)?;
                Ok(Expression {
                    kind: ExpressionKind::YieldFrom(Box::new(operand)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Throw => {
                self.bump()?;
                let operand = self.parse_expr(prec::THROW)?;
                Ok(Expression {
                    kind: ExpressionKind::Throw(Box::new(operand)),
                    span: self.span_since(start),
                })
            }
            TokenKind::AttributeStart => {
                let attributes = self.parse_attributes()?;
                let is_static = self.eat(TokenKind::Static)?;
                self.parse_inline_function(attributes, is_static, start)
            }
            TokenKind::Static if matches!(self.kind2(), TokenKind::Function | TokenKind::Fn) => {
                self.bump()?;
                self.parse_inline_function(Vec::new(), true, start)
            }
            TokenKind::Function | TokenKind::Fn => {
                self.parse_inline_function(Vec::new(), false, start)
            }
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::Exit => self.parse_exit(),
            TokenKind::Isset => {
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
                Ok(Expression {
                    kind: ExpressionKind::Isset(variables),
                    span: self.span_since(start),
                })
            }
            TokenKind::Empty => {
                self.bump()?;
                self.expect(TokenKind::LeftParen, "'('")?;
                let operand = self.parse_expr(0)?;
                self.expect(TokenKind::RightParen, "')'")?;
                Ok(Expression {
                    kind: ExpressionKind::Empty(Box::new(operand)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Eval => {
                self.bump()?;
                self.expect(TokenKind::LeftParen, "'('")?;
                let operand = self.parse_expr(0)?;
                self.expect(TokenKind::RightParen, "')'")?;
                Ok(Expression {
                    kind: ExpressionKind::Eval(Box::new(operand)),
                    span: self.span_since(start),
                })
            }
            TokenKind::Increment | TokenKind::Decrement => {
                let increment = self.kind() == TokenKind::Increment;
                self.bump()?;
                let target = self.parse_prefix()?;
                if !is_assignable(&target.kind) {
                    return Err(self.error("can only increment or decrement variables"));
                }
                Ok(Expression {
                    kind: ExpressionKind::IncDec {
                        target: Box::new(target),
                        increment,
                        prefix: true,
                    },
                    span: self.span_since(start),
                })
            }
            TokenKind::New => {
                let expression = self.parse_new()?;
                self.parse_postfix_onto(expression)
            }
            TokenKind::Clone => {
                let expression = self.parse_clone()?;
                self.parse_postfix_onto(expression)
            }
            _ => {
                // A primary expression followed by postfix operators:
                // `[...]`, `->`, `?->`, `::`, `(...)`, `++`/`--`.
                let expr = self.parse_primary()?;
                self.parse_postfix_onto(expr)
            }
        }
    }

    /// Applies postfix operators (`[...]`, `->`, `?->`, `::`, `(...)`,
    /// `++`/`--`) to an already-parsed expression.
    fn parse_postfix_onto(&mut self, mut expr: Expression) -> Result<Expression, ParseError> {
        loop {
            match self.kind() {
                TokenKind::LeftBracket => {
                    let start = expr.span;
                    self.bump()?;
                    let index = if self.at(TokenKind::RightBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr(0)?))
                    };
                    self.expect(TokenKind::RightBracket, "']'")?;
                    expr = Expression {
                        kind: ExpressionKind::Index {
                            target: Box::new(expr),
                            index,
                        },
                        span: self.span_since(start),
                    };
                }
                TokenKind::Arrow | TokenKind::NullsafeArrow => {
                    let nullsafe = self.kind() == TokenKind::NullsafeArrow;
                    let start = expr.span;
                    self.bump()?;
                    let name = self.parse_member_name()?;
                    if self.at(TokenKind::LeftParen) {
                        let arguments = self.parse_argument_list()?;
                        expr = Expression {
                            kind: ExpressionKind::MethodCall {
                                target: Box::new(expr),
                                name,
                                arguments,
                                nullsafe,
                            },
                            span: self.span_since(start),
                        };
                    } else {
                        expr = Expression {
                            kind: ExpressionKind::Property {
                                target: Box::new(expr),
                                name,
                                nullsafe,
                            },
                            span: self.span_since(start),
                        };
                    }
                }
                TokenKind::DoubleColon => {
                    let start = expr.span;
                    self.bump()?;
                    if self.kind() == TokenKind::Variable || self.kind() == TokenKind::Dollar {
                        let var_expr = self.parse_simple_variable()?;
                        if self.at(TokenKind::LeftParen) {
                            let arguments = self.parse_argument_list()?;
                            expr = Expression {
                                kind: ExpressionKind::StaticCall {
                                    class: Box::new(expr),
                                    name: MemberName::Expression(Box::new(var_expr)),
                                    arguments,
                                },
                                span: self.span_since(start),
                            };
                        } else {
                            let ExpressionKind::Variable(variable) = var_expr.into_kind() else {
                                return Err(self.error("expected a variable after '::'"));
                            };
                            expr = Expression {
                                kind: ExpressionKind::StaticProperty {
                                    class: Box::new(expr),
                                    name: variable,
                                    name_start: var_expr.span.start,
                                },
                                span: self.span_since(start),
                            };
                        }
                    } else {
                        let name = self.parse_member_name()?;
                        if self.at(TokenKind::LeftParen) {
                            let arguments = self.parse_argument_list()?;
                            expr = Expression {
                                kind: ExpressionKind::StaticCall {
                                    class: Box::new(expr),
                                    name,
                                    arguments,
                                },
                                span: self.span_since(start),
                            };
                        } else {
                            expr = Expression {
                                kind: ExpressionKind::ClassConstant {
                                    class: Box::new(expr),
                                    name,
                                },
                                span: self.span_since(start),
                            };
                        }
                    }
                }
                TokenKind::LeftParen => {
                    let start = expr.span;
                    let opening = self.peek_span().start;
                    let arguments = self.parse_argument_list()?;
                    expr = Expression {
                        kind: ExpressionKind::Call {
                            callee: Box::new(expr),
                            arguments,
                            opening,
                        },
                        span: self.span_since(start),
                    };
                }
                TokenKind::Increment | TokenKind::Decrement => {
                    let increment = self.kind() == TokenKind::Increment;
                    let start = expr.span;
                    self.bump()?;
                    if !is_assignable(&expr.kind) {
                        return Err(self.error("can only increment or decrement variables"));
                    }
                    expr = Expression {
                        kind: ExpressionKind::IncDec {
                            target: Box::new(expr),
                            increment,
                            prefix: false,
                        },
                        span: self.span_since(start),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// Literals, names, variables, arrays and parenthesised expressions.
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let start = self.peek_span();
        let kind = self.kind();

        let expression = match kind {
            TokenKind::Integer => {
                let token = self.bump()?;
                let text = self.text_of(&token).to_string();
                Expression {
                    kind: ExpressionKind::Literal(parse_int_literal(&text, token.span)?),
                    span: token.span,
                }
            }
            TokenKind::Float => {
                let token = self.bump()?;
                let text: String = self.text_of(&token).chars().filter(|&c| c != '_').collect();
                let value = text.parse::<f64>().map_err(|_| {
                    ParseError::new(format!("invalid numeric literal \"{text}\""), token.span)
                })?;
                Expression {
                    kind: ExpressionKind::Literal(Literal::Float(value)),
                    span: token.span,
                }
            }
            TokenKind::String => return self.parse_string_token(),
            TokenKind::DoubleQuote => return self.parse_interpolated_string(),
            TokenKind::StartHeredoc => return self.parse_heredoc(),
            TokenKind::True => literal_expr(Literal::Bool(true), self.bump()?.span),
            TokenKind::False => literal_expr(Literal::Bool(false), self.bump()?.span),
            TokenKind::Null => literal_expr(Literal::Null, self.bump()?.span),
            TokenKind::MagicLine => magic(MagicConstant::Line, self.bump()?.span),
            TokenKind::MagicFile => magic(MagicConstant::File, self.bump()?.span),
            TokenKind::MagicDir => magic(MagicConstant::Dir, self.bump()?.span),
            TokenKind::MagicClass => magic(MagicConstant::Class, self.bump()?.span),
            TokenKind::MagicFunction => magic(MagicConstant::Function, self.bump()?.span),
            TokenKind::MagicMethod => magic(MagicConstant::Method, self.bump()?.span),
            TokenKind::MagicNamespace => magic(MagicConstant::Namespace, self.bump()?.span),
            TokenKind::MagicTrait => magic(MagicConstant::Trait, self.bump()?.span),
            TokenKind::MagicProperty => magic(MagicConstant::Property, self.bump()?.span),
            TokenKind::Variable | TokenKind::Dollar => self.parse_simple_variable()?,
            TokenKind::Identifier
            | TokenKind::QualifiedName
            | TokenKind::FullyQualifiedName
            | TokenKind::RelativeName => {
                let name = self.parse_name()?;
                Expression {
                    kind: ExpressionKind::Name(name),
                    span: self.span_since(start),
                }
            }
            // `readonly` and `static` are usable as plain names (`readonly(1)`,
            // `static::foo()`).
            TokenKind::Readonly | TokenKind::Static => {
                let token = self.bump()?;
                let text = self.text_of(&token).to_ascii_lowercase();
                Expression {
                    kind: ExpressionKind::Name(phera_ast::Name {
                        kind: phera_ast::NameKind::Unqualified,
                        text,
                        span: token.span,
                    }),
                    span: token.span,
                }
            }
            TokenKind::Array => self.parse_array(true)?,
            TokenKind::LeftBracket => self.parse_array(false)?,
            // `list(...)` is only an expression as the left side of `=`
            // (foreach values and array-pair values use their own paths).
            TokenKind::List => {
                let target = self.parse_list_target()?;
                if !self.at(TokenKind::Assign) {
                    return Err(self.error_at(
                        target.span,
                        "syntax error: unexpected token, expecting \"=\"",
                    ));
                }
                target
            }
            TokenKind::LeftParen => {
                self.bump()?;
                let inner = self.parse_expr(0)?;
                self.expect(TokenKind::RightParen, "')'")?;
                Expression {
                    kind: inner.into_kind(),
                    span: self.span_since(start),
                }
            }
            _ => {
                return Err(self.error(format!("unexpected {}", self.describe_current())));
            }
        };

        let _ = kind;
        Ok(expression)
    }

    /// `array(...)` or `[...]`. A `[...]` directly followed by `=` is a
    /// destructuring target and becomes a `List`.
    fn parse_array(&mut self, long: bool) -> Result<Expression, ParseError> {
        let start = self.peek_span();
        self.bump()?; // `array` or `[`
        if long {
            self.expect(TokenKind::LeftParen, "'('")?;
        }
        let close = if long {
            TokenKind::RightParen
        } else {
            TokenKind::RightBracket
        };
        let pairs = self.parse_array_pairs(close)?;
        self.expect(close, if long { "')'" } else { "']'" })?;

        if !long && self.at(TokenKind::Assign) {
            return Ok(Expression {
                kind: ExpressionKind::List(array_into_list(pairs)),
                span: self.span_since(start),
            });
        }

        Ok(Expression {
            kind: ExpressionKind::Array(pairs.into_iter().flatten().collect()),
            span: self.span_since(start),
        })
    }

    /// Comma-separated array pairs up to (not including) `close`; empty slots
    /// become `None`, and a trailing comma is allowed.
    pub(crate) fn parse_array_pairs(
        &mut self,
        close: TokenKind,
    ) -> Result<Vec<Option<ArrayItem>>, ParseError> {
        let mut items: Vec<Option<ArrayItem>> = Vec::new();

        loop {
            if self.at(close) {
                break;
            }
            if self.at(TokenKind::Comma) {
                items.push(None);
                self.bump()?;
                continue;
            }
            items.push(Some(self.parse_array_pair()?));
            if !self.eat(TokenKind::Comma)? {
                break;
            }
        }

        Ok(items)
    }

    fn parse_array_pair(&mut self) -> Result<ArrayItem, ParseError> {
        let start = self.peek_span();

        if self.eat(TokenKind::Ellipsis)? {
            let value = self.parse_expr(0)?;
            return Ok(ArrayItem {
                key: None,
                value,
                by_reference: false,
                spread: true,
                span: self.span_since(start),
            });
        }

        if self.at(TokenKind::Ampersand) {
            self.bump()?;
            let value = self.parse_assign_ref_rhs()?;
            return Ok(ArrayItem {
                key: None,
                value,
                by_reference: true,
                spread: false,
                span: self.span_since(start),
            });
        }

        // A `list(...)` value may itself be an assignment target:
        // `['k' => list($a) = $x]`.
        if self.at(TokenKind::List) {
            let target = self.parse_list_target()?;
            let value = if self.eat(TokenKind::Assign)? {
                let span = self.span_since(target.span);
                let rhs = self.parse_expr(0)?;
                Expression {
                    kind: ExpressionKind::Assign {
                        target: Box::new(target),
                        value: Box::new(rhs),
                    },
                    span,
                }
            } else {
                target
            };
            return Ok(ArrayItem {
                key: None,
                value,
                by_reference: false,
                spread: false,
                span: self.span_since(start),
            });
        }

        let key = self.parse_expr(0)?;

        if self.eat(TokenKind::FatArrow)? {
            let by_reference = if self.at(TokenKind::Ampersand) {
                self.bump()?;
                true
            } else {
                false
            };
            // `['k' => list(...)]` and `['k' => list(...) = $x]`
            let value = if self.at(TokenKind::List) {
                let target = self.parse_list_target()?;
                if self.eat(TokenKind::Assign)? {
                    let span = self.span_since(target.span);
                    let rhs = self.parse_expr(0)?;
                    Expression {
                        kind: ExpressionKind::Assign {
                            target: Box::new(target),
                            value: Box::new(rhs),
                        },
                        span,
                    }
                } else {
                    target
                }
            } else {
                self.parse_expr(0)?
            };
            if by_reference && !is_assignable(&value.kind) {
                return Err(self.error("cannot take a reference to this expression"));
            }
            Ok(ArrayItem {
                key: Some(key),
                value,
                by_reference,
                spread: false,
                span: self.span_since(start),
            })
        } else {
            Ok(ArrayItem {
                key: None,
                value: key,
                by_reference: false,
                spread: false,
                span: self.span_since(start),
            })
        }
    }

    /// `list(...)`: a destructuring target.
    pub(crate) fn parse_list_target(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::List, "'list'")?.span;
        self.expect(TokenKind::LeftParen, "'('")?;
        let pairs = self.parse_array_pairs(TokenKind::RightParen)?;
        self.expect(TokenKind::RightParen, "')'")?;
        Ok(Expression {
            kind: ExpressionKind::List(array_into_list(pairs)),
            span: self.span_since(start),
        })
    }

    /// The member after `->` or `::`: an identifier (any keyword), `{ expr }`,
    /// or a variable.
    pub(crate) fn parse_member_name(&mut self) -> Result<MemberName, ParseError> {
        if self.at(TokenKind::LeftBrace) {
            self.bump()?;
            let expression = self.parse_expr(0)?;
            self.expect(TokenKind::RightBrace, "'}'")?;
            return Ok(MemberName::Expression(Box::new(expression)));
        }

        if is_word_token(self.kind()) {
            let span = self.peek_span();
            let name = self.text().to_string();
            self.bump()?;
            return Ok(MemberName::Identifier(phera_ast::Identifier { name, span }));
        }

        if self.kind() == TokenKind::Variable || self.kind() == TokenKind::Dollar {
            let expression = self.parse_simple_variable()?;
            return Ok(MemberName::Expression(Box::new(expression)));
        }

        Err(self.error(format!(
            "expected a member name, found {}",
            self.describe_current()
        )))
    }

    /// `simple_variable`: `$name`, `$$name`, `${ expr }`.
    pub(crate) fn parse_simple_variable(&mut self) -> Result<Expression, ParseError> {
        let start = self.peek_span();

        match self.kind() {
            TokenKind::Variable => {
                let token = self.bump()?;
                let name = self.text_of(&token)[1..].to_string();
                Ok(Expression {
                    kind: ExpressionKind::Variable(Variable::Named(name)),
                    span: token.span,
                })
            }
            TokenKind::Dollar => {
                self.bump()?;
                if self.at(TokenKind::LeftBrace) {
                    self.bump()?;
                    let inner = self.parse_expr(0)?;
                    self.expect(TokenKind::RightBrace, "'}'")?;
                    Ok(Expression {
                        kind: ExpressionKind::Variable(Variable::Dynamic(Box::new(inner))),
                        span: self.span_since(start),
                    })
                } else {
                    let inner = self.parse_simple_variable()?;
                    Ok(Expression {
                        kind: ExpressionKind::Variable(Variable::Dynamic(Box::new(inner))),
                        span: self.span_since(start),
                    })
                }
            }
            _ => Err(self.error(format!(
                "expected a variable, found {}",
                self.describe_current()
            ))),
        }
    }

    /// The right side of `$a = &$b`: a variable.
    fn parse_assign_ref_rhs(&mut self) -> Result<Expression, ParseError> {
        let expression = self.parse_prefix()?;
        if !is_assignable(&expression.kind) {
            return Err(self.error("expected a variable after '&'"));
        }
        Ok(expression)
    }

    /// `yield`, `yield $v`, `yield $k => $v`.
    fn parse_yield(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::Yield, "'yield'")?.span;
        self.saw_yield = true;

        if !self.can_start_expr() {
            return Ok(Expression {
                kind: ExpressionKind::Yield {
                    key: None,
                    value: None,
                },
                span: self.span_since(start),
            });
        }

        let key = self.parse_expr(prec::YIELD + 1)?;
        let value = if self.eat(TokenKind::FatArrow)? {
            Some(Box::new(self.parse_expr(prec::YIELD + 1)?))
        } else {
            None
        };

        Ok(Expression {
            kind: ExpressionKind::Yield {
                key: Some(Box::new(key)),
                value,
            },
            span: self.span_since(start),
        })
    }

    /// `clone $a`, `clone($a)`, `clone($a, [...])` and, as a plain call,
    /// `clone(...)` with named or extra arguments.
    fn parse_clone(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::Clone, "'clone'")?.span;

        if !self.at(TokenKind::LeftParen) {
            let object = self.parse_expr(prec::CLONE)?;
            return Ok(Expression {
                kind: ExpressionKind::Clone {
                    object: Box::new(object),
                    with: None,
                },
                span: self.span_since(start),
            });
        }

        let opening = self.peek_span().start;
        let arguments = self.parse_argument_list()?;
        let kind = match arguments {
            Arguments::List(items)
                if (1..=2).contains(&items.len())
                    && items
                        .iter()
                        .all(|argument| argument.name.is_none() && !argument.spread) =>
            {
                let mut items = items.into_iter();
                let Some(first) = items.next() else {
                    return Err(self.error("clone expects an object"));
                };
                ExpressionKind::Clone {
                    object: Box::new(first.value),
                    with: items.next().map(|item| Box::new(item.value)),
                }
            }
            arguments => ExpressionKind::Call {
                callee: Box::new(clone_callee()),
                opening,
                arguments,
            },
        };

        Ok(Expression {
            kind,
            span: self.span_since(start),
        })
    }

    /// `new`: a class name reference, a computed target, or an anonymous class.
    fn parse_new(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::New, "'new'")?.span;

        let attributes = if self.at(TokenKind::AttributeStart) {
            self.parse_attributes()?
        } else {
            Vec::new()
        };

        let mut modifiers = Modifiers::default();
        loop {
            let modifier = match self.kind() {
                TokenKind::Abstract => "abstract",
                TokenKind::Final => "final",
                TokenKind::Readonly => {
                    if modifiers.is_readonly {
                        return Err(ParseError::compile(
                            "Multiple readonly modifiers are not allowed",
                            self.peek_span(),
                        ));
                    }
                    self.bump()?;
                    modifiers.is_readonly = true;
                    continue;
                }
                _ => break,
            };
            return Err(ParseError::compile(
                format!("Cannot use the {modifier} modifier on an anonymous class"),
                self.peek_span(),
            ));
        }

        if modifiers.is_readonly || self.at(TokenKind::Class) {
            let mut arguments = Arguments::List(vec![]);
            let class = self.parse_anonymous_class(attributes, modifiers, &mut arguments)?;
            return Ok(Expression {
                kind: ExpressionKind::New {
                    class: NewTarget::Anonymous(Box::new(class)),
                    arguments,
                },
                span: self.span_since(start),
            });
        }

        let mut class = match self.kind() {
            TokenKind::Static => {
                let token = self.bump()?;
                Expression {
                    kind: ExpressionKind::Name(phera_ast::Name {
                        kind: phera_ast::NameKind::Unqualified,
                        text: "static".to_string(),
                        span: token.span,
                    }),
                    span: token.span,
                }
            }
            TokenKind::Identifier
            | TokenKind::QualifiedName
            | TokenKind::FullyQualifiedName
            | TokenKind::RelativeName => {
                let name = self.parse_name()?;
                Expression {
                    kind: ExpressionKind::Name(name.clone()),
                    span: name.span,
                }
            }
            TokenKind::LeftParen => {
                self.bump()?;
                let inner = self.parse_expr(0)?;
                self.expect(TokenKind::RightParen, "')'")?;
                inner
            }
            TokenKind::Variable | TokenKind::Dollar => self.parse_new_target_variable()?,
            _ => {
                return Err(self.error(format!(
                    "expected a class name after 'new', found {}",
                    self.describe_current()
                )));
            }
        };

        // `new Foo::$var`, `new static::$var`
        if self.at(TokenKind::DoubleColon) {
            let span = class.span;
            self.bump()?;
            let var_expr = self.parse_simple_variable()?;
            let name_start = var_expr.span.start;
            let ExpressionKind::Variable(variable) = var_expr.into_kind() else {
                return Err(self.error("expected a variable after '::'"));
            };
            let property = Expression {
                kind: ExpressionKind::StaticProperty {
                    class: Box::new(class),
                    name: variable,
                    name_start,
                },
                span: self.span_since(span),
            };
            // `new static::$classes[$key]`: the static property is a
            // variable like any other, so offsets and properties may follow.
            class = self.parse_new_target_steps(property)?;
        }

        let arguments = if self.at(TokenKind::LeftParen) {
            // `new` and the class name are one symbol more than a call's callee.
            self.with_php_slots(1, Self::parse_argument_list)?
        } else {
            Arguments::List(vec![])
        };

        Ok(Expression {
            kind: ExpressionKind::New {
                class: NewTarget::Class(Box::new(class)),
                arguments,
            },
            span: self.span_since(start),
        })
    }

    /// `new $a`, `new $a[0]`, `new $a->b`, `new $a::$b`.
    fn parse_new_target_variable(&mut self) -> Result<Expression, ParseError> {
        let variable = self.parse_simple_variable()?;
        self.parse_new_target_steps(variable)
    }

    /// The offsets, properties and static properties that may follow the
    /// variable naming the class: `[0]`, `->b`, `::$b`, in any number. A call
    /// is not one of them: the first `(` starts the constructor's arguments.
    fn parse_new_target_steps(&mut self, variable: Expression) -> Result<Expression, ParseError> {
        let mut expr = variable;

        loop {
            match self.kind() {
                TokenKind::LeftBracket => {
                    let start = expr.span;
                    self.bump()?;
                    let index = if self.at(TokenKind::RightBracket) {
                        None
                    } else {
                        Some(Box::new(self.parse_expr(0)?))
                    };
                    self.expect(TokenKind::RightBracket, "']'")?;
                    expr = Expression {
                        kind: ExpressionKind::Index {
                            target: Box::new(expr),
                            index,
                        },
                        span: self.span_since(start),
                    };
                }
                TokenKind::Arrow | TokenKind::NullsafeArrow => {
                    let nullsafe = self.kind() == TokenKind::NullsafeArrow;
                    let start = expr.span;
                    self.bump()?;
                    let name = self.parse_member_name()?;
                    expr = Expression {
                        kind: ExpressionKind::Property {
                            target: Box::new(expr),
                            name,
                            nullsafe,
                        },
                        span: self.span_since(start),
                    };
                }
                TokenKind::DoubleColon => {
                    let start = expr.span;
                    self.bump()?;
                    let var_expr = self.parse_simple_variable()?;
                    let name_start = var_expr.span.start;
                    let ExpressionKind::Variable(variable) = var_expr.into_kind() else {
                        return Err(self.error("expected a variable after '::'"));
                    };
                    expr = Expression {
                        kind: ExpressionKind::StaticProperty {
                            class: Box::new(expr),
                            name: variable,
                            name_start,
                        },
                        span: self.span_since(start),
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    /// `exit`, `exit()`, `exit(1)`, `die('x')`.
    fn parse_exit(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::Exit, "'exit'")?.span;

        let value = if self.at(TokenKind::LeftParen) {
            match self.parse_argument_list()? {
                Arguments::List(mut items) => match items.as_slice() {
                    [] => None,
                    [one] if one.name.is_none() && !one.spread => {
                        items.pop().map(|item| Box::new(item.value))
                    }
                    _ => return Err(self.error_at(start, "exit() accepts at most one argument")),
                },
                // `exit(...)` / `die(...)` parse in PHP as a call; the AST has
                // no exit-callable, so it becomes a bare `exit`.
                Arguments::FirstClassCallable => None,
            }
        } else {
            None
        };

        Ok(Expression {
            kind: ExpressionKind::Exit(value),
            span: self.span_since(start),
        })
    }

    /// `match (expr) { ... }`.
    fn parse_match_expr(&mut self) -> Result<Expression, ParseError> {
        let start = self.expect(TokenKind::Match, "'match'")?.span;
        let subject = self.parse_paren_expr()?;
        self.expect(TokenKind::LeftBrace, "'{'")?;

        let mut arms = Vec::new();
        while !self.at(TokenKind::RightBrace) && !self.at(TokenKind::Eof) {
            let arm_start = self.peek_span();
            let conditions = if self.eat(TokenKind::Default)? {
                self.eat(TokenKind::Comma)?; // `default, => ...`
                None
            } else {
                let mut conditions = vec![self.parse_expr(0)?];
                while self.eat(TokenKind::Comma)? {
                    if self.at(TokenKind::FatArrow) {
                        break; // trailing comma before `=>`
                    }
                    conditions.push(self.parse_expr(0)?);
                }
                Some(conditions)
            };
            self.expect(TokenKind::FatArrow, "'=>'")?;
            let body = self.parse_expr(0)?;
            arms.push(MatchArm {
                conditions,
                body,
                span: self.span_since(arm_start),
            });
            if !self.eat(TokenKind::Comma)? {
                break;
            }
        }

        self.expect(TokenKind::RightBrace, "'}'")?;
        Ok(Expression {
            kind: ExpressionKind::Match(Box::new(Match { subject, arms })),
            span: self.span_since(start),
        })
    }

    /// Closures and arrow functions: `function ... {}` / `fn ... => ...`,
    /// optionally with attributes and `static`.
    pub(crate) fn parse_inline_function(
        &mut self,
        attributes: Vec<Attribute>,
        is_static: bool,
        start: Span,
    ) -> Result<Expression, ParseError> {
        let declaration_start = self.peek_span();
        let doc_comment = self
            .doc_comment()
            .or_else(|| self.doc_comment_before(start.start))
            .or_else(|| self.declaration_doc_comment(&attributes));
        if self.at(TokenKind::Function) {
            self.bump()?;
            let by_reference = self.eat(TokenKind::Ampersand)?;
            let parameters = self.parse_parameter_list()?;
            let uses = if self.eat(TokenKind::Use)? {
                self.parse_closure_uses()?
            } else {
                Vec::new()
            };
            let return_type = self.parse_return_type()?;
            self.expect(TokenKind::LeftBrace, "'{'")?;
            // A closure's header is ten symbols on PHP's stack (`static` one more).
            let (body, is_generator) = self
                .with_php_slots(10 + u32::from(is_static), |parser| {
                    parser.generator_body(Self::parse_inner_statements)
                })?;
            self.close_brace()?;
            Ok(Expression {
                kind: ExpressionKind::Closure(Box::new(Closure {
                    declaration_span: self.span_since(declaration_start),
                    doc_comment,
                    attributes,
                    is_static,
                    by_reference,
                    is_generator,
                    parameters,
                    uses,
                    return_type,
                    body,
                })),
                span: self.span_since(start),
            })
        } else if self.at(TokenKind::Fn) {
            self.bump()?;
            let by_reference = self.eat(TokenKind::Ampersand)?;
            let parameters = self.parse_parameter_list()?;
            let return_type = self.parse_return_type()?;
            self.expect(TokenKind::FatArrow, "'=>'")?;
            // An arrow function's header is nine symbols on PHP's stack.
            let (body, is_generator) = self.with_php_slots(9 + u32::from(is_static), |parser| {
                parser.generator_body(|parser| parser.parse_expr(0))
            })?;
            Ok(Expression {
                kind: ExpressionKind::ArrowFunction(Box::new(ArrowFunction {
                    declaration_span: self.span_since(declaration_start),
                    doc_comment,
                    attributes,
                    is_static,
                    by_reference,
                    is_generator,
                    parameters,
                    return_type,
                    body,
                })),
                span: self.span_since(start),
            })
        } else {
            Err(self.error(format!(
                "expected 'function' or 'fn', found {}",
                self.describe_current()
            )))
        }
    }

    /// `use ($a, &$b)` on a closure.
    fn parse_closure_uses(&mut self) -> Result<Vec<ClosureUse>, ParseError> {
        self.expect(TokenKind::LeftParen, "'('")?;
        let mut uses = Vec::new();

        if !self.at(TokenKind::RightParen) {
            loop {
                let start = self.peek_span();
                let by_reference = self.eat(TokenKind::Ampersand)?;
                let token = self.expect(TokenKind::Variable, "a variable")?;
                let name = self.text_of(&token)[1..].to_string();
                uses.push(ClosureUse {
                    name,
                    by_reference,
                    span: self.span_since(start),
                });
                if !self.eat(TokenKind::Comma)? {
                    break;
                }
                if self.at(TokenKind::RightParen) {
                    break;
                }
            }
        }

        self.expect(TokenKind::RightParen, "')'")?;
        Ok(uses)
    }

    /// `: type` after a parameter list, if present.
    pub(crate) fn parse_return_type(&mut self) -> Result<Option<phera_ast::Type>, ParseError> {
        if self.eat(TokenKind::Colon)? {
            Ok(Some(self.parse_type_expr(true)?))
        } else {
            Ok(None)
        }
    }

    /// `(` expr `)`.
    pub(crate) fn parse_paren_expr(&mut self) -> Result<Expression, ParseError> {
        self.expect(TokenKind::LeftParen, "'('")?;
        let expression = self.parse_expr(0)?;
        self.expect(TokenKind::RightParen, "')'")?;
        Ok(expression)
    }

    /// `(...)`: an argument list, or `foo(...)` as a first-class callable.
    pub(crate) fn parse_argument_list(&mut self) -> Result<Arguments, ParseError> {
        self.expect(TokenKind::LeftParen, "'('")?;

        if self.at(TokenKind::RightParen) {
            self.bump()?;
            return Ok(Arguments::List(vec![]));
        }

        if self.at(TokenKind::Ellipsis) && self.at2(TokenKind::RightParen) {
            self.bump()?;
            self.bump()?;
            return Ok(Arguments::FirstClassCallable);
        }

        // PHP's stack holds the callee and the `(` while an argument is parsed.
        let mut arguments = vec![self.with_php_slots(1, Self::parse_argument)?];
        while self.eat(TokenKind::Comma)? {
            if self.at(TokenKind::RightParen) {
                break;
            }
            arguments.push(self.with_php_slots(1, Self::parse_argument)?);
        }
        self.expect(TokenKind::RightParen, "')'")?;

        Ok(Arguments::List(arguments))
    }

    fn parse_argument(&mut self) -> Result<Argument, ParseError> {
        let start = self.peek_span();

        if is_word_token(self.kind()) && self.at2(TokenKind::Colon) {
            let name = self.parse_identifier()?;
            self.bump()?; // `:`
            let value = self.parse_expr(0)?;
            return Ok(Argument {
                name: Some(name),
                value,
                spread: false,
                span: self.span_since(start),
            });
        }

        if self.eat(TokenKind::Ellipsis)? {
            let value = self.parse_expr(0)?;
            return Ok(Argument {
                name: None,
                value,
                spread: true,
                span: self.span_since(start),
            });
        }

        let value = self.parse_expr(0)?;
        Ok(Argument {
            name: None,
            value,
            spread: false,
            span: self.span_since(start),
        })
    }

    /// Whether the current token can start an expression (used by `yield`).
    pub(crate) fn can_start_expr(&self) -> bool {
        use TokenKind::*;
        matches!(
            self.kind(),
            Variable
                | Dollar
                | Integer
                | Float
                | String
                | DoubleQuote
                | StartHeredoc
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
                | Identifier
                | QualifiedName
                | FullyQualifiedName
                | RelativeName
                | Readonly
                | Static
                | New
                | Clone
                | Fn
                | Function
                | Match
                | Exit
                | Isset
                | Empty
                | Eval
                | List
                | Array
                | LeftBracket
                | LeftParen
                | Plus
                | Minus
                | Not
                | Tilde
                | At
                | IntCast
                | FloatCast
                | StringCast
                | ArrayCast
                | ObjectCast
                | BoolCast
                | UnsetCast
                | VoidCast
                | Include
                | IncludeOnce
                | Require
                | RequireOnce
                | Print
                | Yield
                | YieldFrom
                | Throw
                | Increment
                | Decrement
                | AttributeStart
        )
    }
}

fn unary(operator: UnaryOperator, operand: Expression, span: Span) -> Expression {
    Expression {
        kind: ExpressionKind::Unary {
            operator,
            operand: Box::new(operand),
        },
        span,
    }
}

fn literal_expr(literal: Literal, span: Span) -> Expression {
    Expression {
        kind: ExpressionKind::Literal(literal),
        span,
    }
}

fn magic(constant: MagicConstant, span: Span) -> Expression {
    Expression {
        kind: ExpressionKind::MagicConstant(constant),
        span,
    }
}

fn cast_kind(kind: TokenKind) -> CastKind {
    match kind {
        TokenKind::IntCast => CastKind::Int,
        TokenKind::FloatCast => CastKind::Float,
        TokenKind::StringCast => CastKind::String,
        TokenKind::ArrayCast => CastKind::Array,
        TokenKind::ObjectCast => CastKind::Object,
        TokenKind::BoolCast => CastKind::Bool,
        TokenKind::UnsetCast => CastKind::Unset,
        _ => CastKind::Void,
    }
}

/// PHP parses `clone(...)` with unusual arguments as a plain call to `\clone`.
fn clone_callee() -> Expression {
    Expression {
        kind: ExpressionKind::Name(phera_ast::Name {
            kind: phera_ast::NameKind::FullyQualified,
            text: "clone".to_string(),
            span: Span::new(0, 0),
        }),
        span: Span::new(0, 0),
    }
}

/// Converts array items into list items, recursively: nested arrays in a
/// destructuring target are lists too (`['k' => [$a, $b]] = $x`).
pub(crate) fn array_into_list(items: Vec<Option<ArrayItem>>) -> Vec<Option<ArrayItem>> {
    items
        .into_iter()
        .map(|item| {
            item.map(|mut item| {
                if let ExpressionKind::Array(inner) = &mut item.value.kind {
                    let inner = std::mem::take(inner);
                    item.value.kind = ExpressionKind::List(array_into_list(
                        inner.into_iter().map(Some).collect(),
                    ));
                }
                item
            })
        })
        .collect()
}

fn hex_digit_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => byte - b'A' + 10,
    }
}

/// Parses an integer literal token's text: decimal, hex, octal (`0o` or
/// leading zero) or binary, with `_` separators. Values that do not fit in
/// `i64` become floats, as in PHP.
fn parse_int_literal(text: &str, span: Span) -> Result<Literal, ParseError> {
    let cleaned: String = text.chars().filter(|&c| c != '_').collect();
    let bytes = cleaned.as_bytes();

    let (radix, digits): (u32, &str) = if bytes.len() >= 2 && bytes[0] == b'0' {
        match bytes[1] {
            b'x' | b'X' => (16, &cleaned[2..]),
            b'o' | b'O' => (8, &cleaned[2..]),
            b'b' | b'B' => (2, &cleaned[2..]),
            b'0'..=b'9' => (8, cleaned.as_str()),
            _ => (10, cleaned.as_str()),
        }
    } else {
        (10, cleaned.as_str())
    };

    match u128::from_str_radix(digits, radix) {
        Ok(value) if value <= i64::MAX as u128 => Ok(Literal::Int(value as i64)),
        Ok(value) => Ok(Literal::Float(value as f64)),
        // Too big even for u128 (or an invalid octal digit): PHP makes it a
        // float, except for octal digits 8/9 which are a lex error.
        Err(error) => {
            if *error.kind() == std::num::IntErrorKind::InvalidDigit {
                return Err(ParseError::new(
                    format!("invalid numeric literal \"{text}\""),
                    span,
                ));
            }
            let mut value = 0f64;
            for digit in digits.bytes() {
                value = value * radix as f64 + f64::from(hex_digit_value(digit));
            }
            Ok(Literal::Float(value))
        }
    }
}
