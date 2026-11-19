//! Block checks detect `return; echo 1;` without control-flow or type inference.

use super::{Context, Node, condition, variable};
use crate::lint::{Language, source::is_comment};
use phera_ast::{ExpressionKind as Expr, Literal, Statement, StatementKind as Stmt};
use phera_source::Span;

pub(super) fn assignment_in_condition(context: &mut Context<'_>, node: Node<'_>) {
    if let Some(condition) = condition(node)
        && let Expr::Assign { target, value } = &condition.kind
        && variable(target).is_some()
        && matches!(value.kind, Expr::Literal(_))
    {
        context.report(
            "assignment-in-condition",
            condition.span,
            "Assignment used as a condition. Did you mean to compare values?",
        );
    }
}

pub(super) fn constant_condition(context: &mut Context<'_>, node: Node<'_>) {
    if let Some(condition) = condition(node)
        && let Expr::Literal(literal) = &condition.kind
    {
        let truthy = match literal {
            Literal::Bool(value) => *value,
            Literal::Null => false,
            Literal::Int(n) => *n != 0,
            Literal::Float(n) => *n != 0.0,
            Literal::String(bytes) => !bytes.is_empty() && bytes != b"0",
        };
        context.report(
            "constant-condition",
            condition.span,
            format!("This condition is always {truthy}."),
        );
    }
}

pub(super) fn identical_branches(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::If {
        then,
        otherwise: Some(otherwise),
        ..
    } = &statement.kind
    else {
        return;
    };
    let span = |statements: &[Statement]| {
        Some(Span::new(
            statements.first()?.span.start,
            statements.last()?.span.end,
        ))
    };
    if let (Some(a), Some(b)) = (span(then), span(otherwise))
        && a.end.saturating_sub(a.start) == b.end.saturating_sub(b.start)
        && !context.source.slice(a).is_empty()
        && context.source.slice(a) == context.source.slice(b)
        && !context
            .source
            .slice(a)
            .windows(8)
            .any(|text| text.eq_ignore_ascii_case(b"__LINE__"))
    {
        context.report(
            "identical-branches",
            statement.span,
            "Both branches of this if contain the same code.",
        );
    }
}

fn exits(statement: &Statement) -> bool {
    match &statement.kind {
        Stmt::Break(_) | Stmt::Continue(_) | Stmt::Return(_) | Stmt::Goto(_) => true,
        Stmt::Expression(e) => matches!(e.kind, Expr::Throw(_) | Expr::Exit(_)),
        _ => false,
    }
}

pub(super) fn after_exit(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(body) = node else {
        return;
    };
    let mut exited = false;
    for statement in body {
        match &statement.kind {
            Stmt::Label(_) => exited = false,
            // PHP can register declarations independently of statement execution.
            Stmt::Function(_) | Stmt::Class(_) | Stmt::InlineHtml(_) => continue,
            _ if exited => {
                context.report(
                    "unreachable-statement",
                    statement.span,
                    "This statement follows an unconditional exit.",
                );
                exited = false;
            }
            _ => exited = exits(statement),
        }
    }
}

pub(super) fn single_iteration(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let body = match &statement.kind {
        Stmt::While { body, .. }
        | Stmt::DoWhile { body, .. }
        | Stmt::For { body, .. }
        | Stmt::Foreach { body, .. } => body,
        _ => return,
    };
    if body
        .last()
        .is_some_and(|last| exits(last) && !matches!(last.kind, Stmt::Continue(_) | Stmt::Goto(_)))
    {
        // A continue in a nested branch may bypass the final exit.
        let mut pending = vec![body.as_slice()];
        while let Some(block) = pending.pop() {
            for statement in block {
                if matches!(
                    statement.kind,
                    Stmt::Continue(_) | Stmt::Goto(_) | Stmt::Label(_)
                ) {
                    return;
                }
                pending.extend(crate::collect::child_blocks(statement));
            }
        }
        context.report(
            "loop-runs-once",
            statement.span,
            "This loop exits on its first iteration.",
        );
    }
}

pub(super) fn fallthrough(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Switch { cases, .. } = &statement.kind else {
        return;
    };
    for pair in cases.windows(2) {
        let [case, next] = pair else {
            continue;
        };
        let Some(last) = case.body.last() else {
            continue;
        };
        if exits(last) {
            continue;
        }
        let gap = Span::new(last.span.end, next.span.start);
        if !context.source.within(gap).iter().any(is_comment) {
            context.report(
                "switch-fallthrough",
                next.span,
                "The previous case falls through into this one without a comment.",
            );
        }
    }
}

pub(super) fn empty_catch(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Try { catches, .. } = &statement.kind else {
        return;
    };
    for catch in catches {
        if catch.body.is_empty() && !context.source.within(catch.span).iter().any(is_comment) {
            context.report(
                "empty-catch",
                catch.span,
                "This catch discards its exception without a comment.",
            );
        }
    }
}

pub(super) fn rethrow(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Try {
        catches, finally, ..
    } = &statement.kind
    else {
        return;
    };
    let Some(catch) = catches.last().filter(|_| finally.is_none()) else {
        return;
    };
    if let Some(binding) = catch.variable.as_deref()
        && let [only] = catch.body.as_slice()
        && let Stmt::Expression(expr) = &only.kind
        && let Expr::Throw(thrown) = &expr.kind
        && variable(thrown) == Some(binding)
    {
        context.report(
            "rethrow-only-catch",
            catch.span,
            "This catch only rethrows the exception.",
        );
    }
}

pub(super) fn return_in_finally(context: &mut Context<'_>, node: Node<'_>) {
    if context.scope.in_finally
        && let Node::Statement(statement) = node
        && matches!(statement.kind, Stmt::Return(_))
    {
        let message = match context.language {
            Language::Php85 => "Returning from finally overrides a pending return or exception.",
            Language::Php86 => "Returning from finally is deprecated in PHP 8.6.",
        };
        context.report("return-in-finally", statement.span, message);
    }
}

pub(super) fn goto(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Statement(statement) = node
        && let Stmt::Goto(label) = &statement.kind
    {
        context.report(
            "goto",
            statement.span,
            format!("goto jumps directly to '{}'.", label.name),
        );
    }
}

pub(super) fn unused_expression(context: &mut Context<'_>, node: Node<'_>) {
    if let Node::Statement(statement) = node
        && let Stmt::Expression(expr) = &statement.kind
        && matches!(expr.kind, Expr::Literal(_))
    {
        context.report(
            "unused-expression",
            expr.span,
            "This expression has no effect.",
        );
    }
}

pub(super) fn unreachable_catch(context: &mut Context<'_>, node: Node<'_>) {
    use phera_ast::UseKind;
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Try { catches, .. } = &statement.kind else {
        return;
    };
    let Some(index) = context.index else {
        return;
    };
    let mut previous = Vec::<String>::new();
    for catch in catches {
        let mut resolved = Vec::new();
        for ty in &catch.types {
            let resolved_type = context.names.resolve(ty, UseKind::Class);
            let Some(name) = resolved_type.first() else {
                continue;
            };
            let ancestry = index.ancestry(name);
            if let Some(wider) = previous.iter().find(|wider| {
                name.eq_ignore_ascii_case(wider)
                    || ancestry
                        .iter()
                        .any(|class| class.name.eq_ignore_ascii_case(wider))
            }) {
                context.report(
                    "unreachable-catch",
                    ty.span,
                    format!("Catch type '{name}' is already handled by '{wider}'."),
                );
            }
            resolved.push(name.clone());
        }
        previous.extend(resolved);
    }
}

pub(super) fn foreach_reference(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Block(body) = node else {
        return;
    };
    for pair in body.windows(2) {
        let [statement, next] = pair else {
            continue;
        };
        let Stmt::Foreach {
            value,
            by_reference: true,
            ..
        } = &statement.kind
        else {
            continue;
        };
        let Some(name) = variable(value) else {
            continue;
        };
        if matches!(&next.kind, Stmt::Unset(items) if items.iter().any(|item| variable(item) == Some(name)))
            || matches!(next.kind, Stmt::Return(_))
        {
            continue;
        }
        context.report(
            "foreach-reference-not-unset",
            value.span,
            format!("Unset ${name} after foreach to release the last element reference."),
        );
    }
}

pub(super) fn discard_exception(context: &mut Context<'_>, node: Node<'_>) {
    let Node::Statement(statement) = node else {
        return;
    };
    let Stmt::Try { catches, .. } = &statement.kind else {
        return;
    };
    for catch in catches {
        if catch.variable.is_none()
            && !catch.body.is_empty()
            && catch.types.iter().any(|name| {
                context
                    .names
                    .resolve(name, phera_ast::UseKind::Class)
                    .first()
                    .is_some_and(|name| name.eq_ignore_ascii_case("Exception"))
            })
        {
            context.report(
                "discarded-exception",
                catch.span,
                "This handler catches Exception without retaining the exception.",
            );
        }
    }
}
