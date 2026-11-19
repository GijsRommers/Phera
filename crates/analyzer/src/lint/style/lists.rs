//! A multiline call such as `f($value,)` can keep its final comma.

use super::style;
use crate::lint::{
    Context,
    fix::{Edit, Fix},
    rule::{Rule, Visit},
    traversal::Node,
};
use phera_ast::{Arguments, ExpressionKind as Expr};
use phera_lexer::TokenKind as Kind;
use phera_source::Span;

pub(super) const RULES: &[Rule] = &[style(
    "trailing-comma",
    "Use a final comma before the closing delimiter of multiline lists.",
    "$values = [\n    1\n];",
    "$values = [\n    1,\n];",
    "Safe token insertion for PHP lists.",
    Visit::Expression,
    trailing_comma,
)];

fn trailing_comma(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expression, _) = node else {
        return;
    };
    let (last, close) = match &expression.kind {
        Expr::Array(items) => (items.last().map(|item| item.span.end), None),
        Expr::List(items) => (
            items
                .last()
                .and_then(|item| item.as_ref())
                .map(|item| item.span.end),
            None,
        ),
        Expr::Call {
            arguments: Arguments::List(args),
            ..
        }
        | Expr::New {
            arguments: Arguments::List(args),
            ..
        }
        | Expr::MethodCall {
            arguments: Arguments::List(args),
            ..
        }
        | Expr::StaticCall {
            arguments: Arguments::List(args),
            ..
        } => (args.last().map(|arg| arg.span.end), Some(Kind::RightParen)),
        Expr::Match(value) => (
            value.arms.last().map(|arm| arm.body.span.end),
            Some(Kind::RightBrace),
        ),
        _ => return,
    };
    let Some(last) = last else {
        return;
    };
    if context.lines.line(last) == context.lines.line(expression.span.end) {
        return;
    }
    let tail = context.source.within(Span::new(last, expression.span.end));
    let Some(token) = tail.iter().find(|token| {
        !matches!(
            token.kind,
            Kind::Whitespace | Kind::Comment | Kind::DocComment
        )
    }) else {
        return;
    };
    if close.map_or(
        matches!(token.kind, Kind::RightBracket | Kind::RightParen),
        |close| token.kind == close,
    ) && context.lines.line(last) != context.lines.line(token.span.start)
    {
        let span = Span::new(last, last);
        context.with_fix(
            "trailing-comma",
            span,
            "Add a trailing comma to this multiline list.",
            Fix::safe(vec![Edit::replace(span, vec![b','])]),
        );
    }
}
