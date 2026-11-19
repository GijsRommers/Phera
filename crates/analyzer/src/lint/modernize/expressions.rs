//! Use `[$item]` and `$value ?? $fallback` in PHP 8.5.

use super::rule;
use crate::lint::{
    Context,
    fix::{Edit, Fix},
    rule::{DefaultStatus, Rule, Visit},
    source::is_comment,
    traversal::Node,
};
use phera_ast::{
    BinaryOperator as Op, Expression, ExpressionKind as Expr, Literal, MemberName,
    StatementKind as Stmt, Variable,
};
use phera_lexer::TokenKind;
use phera_source::Span;

pub(super) const RULES: &[Rule] = &[
    Rule {
        default: DefaultStatus::OptIn,
        ..rule(
            "short-array-syntax",
            "Use brackets for array literals.",
            "$values = array(1, 2);",
            "$values = [1, 2];",
            "Safe fix. Only the array delimiters change.",
            Visit::Expression,
            short_array,
        )
    },
    Rule {
        default: DefaultStatus::Enabled,
        ..rule(
            "short-list-syntax",
            "Use brackets for array destructuring.",
            "list($a, $b) = $pair;",
            "[$a, $b] = $pair;",
            "Safe fix. Only the list delimiters change.",
            Visit::Expression,
            short_list,
        )
    },
    rule(
        "null-coalescing",
        "Use ?? for an isset ternary that returns the checked variable.",
        "$value = isset($a) ? $a : $b;",
        "$value = $a ?? $b;",
        "Safe fix on a single line without comments.",
        Visit::Expression,
        coalescing,
    ),
    rule(
        "null-coalescing-assignment",
        "Use ??= when assigning a null-coalescing expression back to its variable.",
        "$value = $value ?? 'default';",
        "$value ??= 'default';",
        "Safe fix on a single line without comments.",
        Visit::Expression,
        coalescing_assignment,
    ),
    rule(
        "nullsafe-operator",
        "Use ?-> for a method call guarded by a null check.",
        "$value = $a !== null ? $a->read() : null;",
        "$value = $a?->read();",
        "Unsafe fix. An undefined variable is read once and the guarded call must be a direct dereference.",
        Visit::Expression,
        nullsafe,
    ),
    rule(
        "switch-to-match",
        "Consider match when all switch cases return or assign a value.",
        "switch ($n) { case 1: return 'a'; default: return 'b'; }",
        "return match ($n) { 1 => 'a', default => 'b' };",
        "Review the subject and keys for match's strict comparison.",
        Visit::Statement,
        switch_to_match,
    ),
];

fn short_array(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && matches!(expr.kind, Expr::Array(_))
    {
        short_delimiters(
            context,
            expr,
            b"array",
            TokenKind::Array,
            "short-array-syntax",
        );
    }
}

fn short_list(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Expression(expr, _) = node
        && matches!(expr.kind, Expr::List(_))
    {
        short_delimiters(context, expr, b"list", TokenKind::List, "short-list-syntax");
    }
}

fn short_delimiters(
    context: &mut Context<'_>,
    expr: &Expression,
    spelling: &[u8],
    kind: TokenKind,
    rule: &'static str,
) {
    let bytes = context.source.slice(expr.span);
    let prefix = bytes
        .iter()
        .copied()
        .skip_while(|byte| byte.is_ascii_whitespace() || *byte == b'(');
    if !prefix.take(spelling.len()).eq(spelling
        .iter()
        .copied()
        .map(|byte| byte.to_ascii_lowercase()))
        && !bytes
            .windows(spelling.len())
            .any(|window| window.eq_ignore_ascii_case(spelling))
    {
        return;
    }

    let mut tokens = context
        .source
        .within(expr.span)
        .iter()
        .filter(|t| t.kind != TokenKind::Whitespace);
    let Some(keyword) = tokens
        .by_ref()
        .find(|token| token.kind != TokenKind::LeftParen && !is_comment(token))
    else {
        return;
    };
    if keyword.kind != kind {
        return;
    }
    let significant = tokens.by_ref().find(|token| !is_comment(token));
    let Some(open) = significant.filter(|token| token.kind == TokenKind::LeftParen) else {
        return;
    };

    let mut depth = 1usize;
    for token in tokens {
        match token.kind {
            TokenKind::LeftParen => depth += 1,
            TokenKind::RightParen => {
                depth -= 1;
                if depth == 0 {
                    let edits = vec![
                        // Remove only the keyword and change the two delimiters.
                        Edit::remove(keyword.span),
                        Edit::replace(open.span, b"[".to_vec()),
                        Edit::replace(token.span, b"]".to_vec()),
                    ];
                    context.with_fix(rule, keyword.span, "Use bracket syntax.", Fix::safe(edits));
                    return;
                }
            }
            _ => {}
        }
    }
}

fn coalescing(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Ternary {
        condition,
        then: Some(value),
        otherwise,
    } = &expr.kind
    else {
        return;
    };
    let Expr::Isset(checked) = &condition.kind else {
        return;
    };
    let [checked] = checked.as_slice() else {
        return;
    };
    if !same_variable(checked, value) {
        return;
    }
    report_replacement(
        context,
        "null-coalescing",
        expr.span,
        "Use ?? for this isset ternary.",
        [
            b"(".as_slice(),
            context.source.slice(checked.span),
            b" ?? (",
            context.source.slice(otherwise.span),
            b"))",
        ],
        false,
    );
}

fn coalescing_assignment(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Assign { target, value } = &expr.kind else {
        return;
    };
    let Expr::Binary {
        operator: Op::NullCoalesce,
        left,
        right,
    } = &value.kind
    else {
        return;
    };
    if !same_variable(target, left) {
        return;
    }
    report_replacement(
        context,
        "null-coalescing-assignment",
        expr.span,
        "Use ??= for this assignment.",
        [
            b"(".as_slice(),
            context.source.slice(target.span),
            b" ??= (",
            context.source.slice(right.span),
            b"))",
        ],
        false,
    );
}

fn nullsafe(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Expression(expr, _) = node else {
        return;
    };
    let Expr::Ternary {
        condition,
        then: Some(value),
        otherwise,
    } = &expr.kind
    else {
        return;
    };
    let Expr::Binary {
        operator: Op::NotIdentical,
        left,
        right,
    } = &condition.kind
    else {
        return;
    };
    let filter = if matches!(right.kind, Expr::Literal(Literal::Null)) {
        left
    } else if matches!(left.kind, Expr::Literal(Literal::Null)) {
        right
    } else {
        return;
    };
    if !matches!(otherwise.kind, Expr::Literal(Literal::Null)) {
        return;
    }
    let Expr::MethodCall {
        target,
        name: MemberName::Identifier(_),
        nullsafe: false,
        ..
    } = &value.kind
    else {
        return;
    };
    if !same_variable(filter, target) {
        return;
    }
    let gap = Span::new(target.span.end, value.span.end);
    let Some(arrow) = context
        .source
        .within(gap)
        .iter()
        .find(|token| token.kind == TokenKind::Arrow)
    else {
        return;
    };
    report_replacement(
        context,
        "nullsafe-operator",
        expr.span,
        "Use ?-> for a null-guarded method call.",
        [
            b"(".as_slice(),
            context
                .source
                .slice(Span::new(value.span.start, arrow.span.start)),
            b"?->",
            context
                .source
                .slice(Span::new(arrow.span.end, value.span.end)),
            b")",
        ],
        true,
    );
}

fn switch_to_match(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Switch { cases, .. } = &statement.kind else {
        return;
    };
    if cases.len() < 2 || cases.iter().all(|case| case.condition.is_some()) {
        return;
    }
    let all_return = cases.iter().all(|case| {
        matches!(case.body.as_slice(), [statement] if matches!(statement.kind, Stmt::Return(Some(_))))
    });
    let mut assignee = None;
    let all_assign = cases.iter().all(|case| {
        let [statement, exit] = case.body.as_slice() else {
            return false;
        };
        let Stmt::Expression(expression) = &statement.kind else {
            return false;
        };
        let Expr::Assign { target, .. } = &expression.kind else {
            return false;
        };
        let Expr::Variable(Variable::Named(name)) = &target.kind else {
            return false;
        };
        matches!(exit.kind, Stmt::Break(1))
            && assignee.get_or_insert(name.as_str()) == &name.as_str()
    });
    if all_return || all_assign {
        context.report(
            "switch-to-match",
            statement.span,
            "Consider match after checking that strict comparison is appropriate.",
        );
    }
}

pub(super) fn same_variable(a: &Expression, b: &Expression) -> bool {
    matches!((&a.kind, &b.kind), (Expr::Variable(Variable::Named(a)), Expr::Variable(Variable::Named(b))) if a == b)
}

pub(super) fn report_replacement<const N: usize>(
    context: &mut Context<'_>,
    rule: &'static str,
    span: Span,
    message: &str,
    pieces: [&[u8]; N],
    may_change_behavior: bool,
) {
    if context.lines.line(span.start) != context.lines.line(span.end)
        || context.source.within(span).iter().any(is_comment)
    {
        context.report(rule, span, message);
        return;
    }
    let edits = vec![Edit::replace(span, pieces.concat())];
    let fix = if may_change_behavior {
        Fix::risky(edits)
    } else {
        Fix::safe(edits)
    };
    context.with_fix(rule, span, message, fix);
}
