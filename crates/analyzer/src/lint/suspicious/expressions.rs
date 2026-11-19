//! Expression checks include `$value = $value` and `in_array($key, $keys)`.

use super::{Context, Node, variable};
use phera_ast::{
    Arguments, BinaryOperator as Op, Expression, ExpressionKind as Expr, Literal, UnaryOperator,
    UseKind, Variable,
};

pub(super) fn self_assignment(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && let Expr::Assign { target, value } = &expr.kind
        && let Some(name) = variable(target)
        && variable(value) == Some(name)
    {
        context.report(
            "self-assignment",
            expr.span,
            format!("Variable ${name} is assigned to itself."),
        );
    }
}

pub(super) fn self_comparison(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && let Expr::Binary {
            operator,
            left,
            right,
        } = &expr.kind
        && comparison(*operator)
        && let Some(name) = variable(left)
        && variable(right) == Some(name)
    {
        context.report(
            "self-comparison",
            expr.span,
            format!("Variable ${name} is compared with itself."),
        );
    }
}

pub(super) fn loose_comparison(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Binary {
        operator,
        left,
        right,
    } = &expr.kind
    else {
        return;
    };
    if !matches!(operator, Op::Equal | Op::NotEqual)
        || !matches!(
            (&left.kind, &right.kind),
            (Expr::Literal(_), Expr::Literal(_))
        )
    {
        return;
    }
    let (Expr::Literal(left), Expr::Literal(right)) = (&left.kind, &right.kind) else {
        return;
    };
    let trap = match (left, right) {
        (Literal::String(a), Literal::String(b)) => numeric(a) != numeric(b),
        (Literal::String(_), Literal::Null | Literal::Int(_) | Literal::Float(_))
        | (Literal::Null | Literal::Int(_) | Literal::Float(_), Literal::String(_)) => true,
        _ => false,
    };
    if trap {
        context.report(
            "loose-comparison",
            expr.span,
            "Use a strict comparison for these scalar literals.",
        );
    }
}

fn numeric(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).ok().is_some_and(|text| {
        let text = text.trim_matches(|c: char| c.is_ascii_whitespace());
        text.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'+' | b'-' | b'e' | b'E'))
            && text.parse::<f64>().is_ok()
    })
}

pub(super) fn empty_literal(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && let Expr::Empty(value) = &expr.kind
        && matches!(value.kind, Expr::Literal(_))
    {
        context.report(
            "empty-on-literal",
            expr.span,
            "The truthiness of this literal is already known.",
        );
    }
}

pub(super) fn dynamic_variable(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && matches!(expr.kind, Expr::Variable(Variable::Dynamic(_)))
    {
        context.report(
            "variable-variables",
            expr.span,
            "This variable name is computed at runtime.",
        );
    }
}

pub(super) fn eval(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && matches!(expr.kind, Expr::Eval(_))
    {
        context.report(
            "eval",
            expr.span,
            "Avoid evaluating PHP source dynamically.",
        );
    }
}

pub(super) fn error_suppression(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && let Expr::Unary {
            operator: UnaryOperator::ErrorSuppress,
            ..
        } = expr.kind
    {
        context.report(
            "error-suppression",
            expr.span,
            "The @ operator suppresses PHP errors.",
        );
    }
}

pub(super) fn extract(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && call(context, expr).is_some_and(|(name, _)| name == "extract")
    {
        context.report(
            "extract",
            expr.span,
            "extract() creates local variables using dynamic names.",
        );
    }
}

pub(super) fn array_search(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && let Some((name, arguments)) = call(context, expr)
        && matches!(name.as_str(), "in_array" | "array_search")
        && arguments.len() >= 2
        && !arguments.iter().any(|arg| arg.spread)
    {
        let strict = arguments
            .iter()
            .find(|arg| arg.name.as_ref().is_some_and(|n| n.name == "strict"))
            .or_else(|| arguments.get(2).filter(|arg| arg.name.is_none()));
        if strict.is_none_or(|arg| matches!(arg.value.kind, Expr::Literal(Literal::Bool(false)))) {
            context.report(
                "non-strict-array-search",
                expr.span,
                format!("Pass true as the strict flag for {name}()."),
            );
        }
    }
}

pub(super) fn debug(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    if let Some((name, arguments)) = call(context, expr) {
        let debugging = match name.as_str() {
            "var_dump" | "dd" | "dump" => true,
            "print_r" => {
                arguments.len() == 1
                    || arguments.get(1).is_some_and(|arg| {
                        matches!(arg.value.kind, Expr::Literal(Literal::Bool(false)))
                    })
            }
            "error_log" => arguments
                .first()
                .is_some_and(|arg| matches!(arg.value.kind, Expr::Literal(Literal::String(_)))),
            _ => false,
        };
        if debugging {
            context.report(
                "debug-call",
                expr.span,
                format!("Check whether {name}() was left behind after debugging."),
            );
        }
    } else if matches!(expr.kind, Expr::Exit(_))
        && context
            .source
            .slice(expr.span)
            .get(..3)
            .is_some_and(|name| name.eq_ignore_ascii_case(b"die"))
    {
        context.report(
            "debug-call",
            expr.span,
            "Check whether die() was left behind after debugging.",
        );
    }
}

fn call<'a>(
    context: &Context<'_>,
    expression: &'a Expression,
) -> Option<(String, &'a [phera_ast::Argument])> {
    let Expr::Call {
        callee,
        arguments: Arguments::List(arguments),
    } = &expression.kind
    else {
        return None;
    };
    let Expr::Name(name) = &callee.kind else {
        return None;
    };
    if name.text.contains('\\') {
        return None;
    }
    let names = context.names.resolve(name, UseKind::Function);
    let candidate = names.last()?.to_ascii_lowercase();
    Some((candidate, arguments))
}

fn comparison(op: Op) -> bool {
    matches!(
        op,
        Op::Equal
            | Op::NotEqual
            | Op::Identical
            | Op::NotIdentical
            | Op::Less
            | Op::LessEqual
            | Op::Greater
            | Op::GreaterEqual
            | Op::Spaceship
    )
}
