//! PHP attaches `$a + $b = 1` to the rightmost operand as `$a + ($b = 1)`.
//! Walk that spine explicitly and move the right-hand value into its final place.

use super::is_assignable;
use crate::{ParseError, parser::Parser};
use phera_ast::{BinaryOperator, CastKind, Expression, ExpressionKind, UnaryOperator};
use phera_source::Span;

pub(super) enum Assignment {
    Value(Expression, Option<BinaryOperator>),
    Reference(Expression),
}

enum Outer {
    Binary(BinaryOperator, Box<Expression>),
    Ternary(Box<Expression>, Option<Box<Expression>>),
    Unary(UnaryOperator),
    Cast(CastKind),
    Clone(Option<Box<Expression>>),
}

pub(super) fn attach(
    mut target: Expression,
    assignment: Assignment,
    operator: Span,
    parser: &Parser<'_>,
) -> Result<Expression, ParseError> {
    let compound = matches!(assignment, Assignment::Value(_, Some(_)));
    let mut spine = Vec::new();
    while !is_assignable(&target.kind)
        || (compound && matches!(target.kind, ExpressionKind::List(_)))
    {
        use ExpressionKind::*;
        let span = target.span;
        let (outer, child) = match target.into_kind() {
            Binary {
                operator,
                left,
                right,
            } => (Outer::Binary(operator, left), right),
            Ternary {
                condition,
                then,
                otherwise,
            } => (Outer::Ternary(condition, then), otherwise),
            Unary { operator, operand } => (Outer::Unary(operator), operand),
            Cast { kind, operand } => (Outer::Cast(kind), operand),
            Clone { object, with } => (Outer::Clone(with), object),
            _ => return Err(parser.error_at(
                operator,
                "syntax error: unexpected assignment operator, cannot assign to this expression",
            )),
        };
        spine.push((outer, span));
        target = *child;
    }
    let end = match &assignment {
        Assignment::Value(value, _) | Assignment::Reference(value) => value.span.end,
    };
    let span = Span::new(target.span.start, end);
    let target = Box::new(target);
    let kind = match assignment {
        Assignment::Value(value, None) => ExpressionKind::Assign {
            target,
            value: Box::new(value),
        },
        Assignment::Value(value, Some(operator)) => ExpressionKind::CompoundAssign {
            operator,
            target,
            value: Box::new(value),
        },
        Assignment::Reference(value) => ExpressionKind::AssignReference {
            target,
            value: Box::new(value),
        },
    };
    let mut expression = Expression { kind, span };
    while let Some((outer, span)) = spine.pop() {
        let inner = Box::new(expression);
        let kind = match outer {
            Outer::Binary(operator, left) => ExpressionKind::Binary {
                operator,
                left,
                right: inner,
            },
            Outer::Ternary(condition, then) => ExpressionKind::Ternary {
                condition,
                then,
                otherwise: inner,
            },
            Outer::Unary(operator) => ExpressionKind::Unary {
                operator,
                operand: inner,
            },
            Outer::Cast(kind) => ExpressionKind::Cast {
                kind,
                operand: inner,
            },
            Outer::Clone(with) => ExpressionKind::Clone {
                object: inner,
                with,
            },
        };
        expression = Expression {
            kind,
            span: Span::new(span.start, span.end.max(end)),
        };
    }
    Ok(expression)
}
