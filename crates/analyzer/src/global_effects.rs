//! Global effects are uncertain across impure user calls: `function f() { global $x; }`.
//! A function without calls or global access can preserve the caller's global facts.

use phera_ast::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalEffects {
    None,
    Unknown,
}

pub fn of(body: &[Statement]) -> GlobalEffects {
    let mut pending = vec![body];
    while let Some(block) = pending.pop() {
        for statement in block {
            if matches!(statement.kind, StatementKind::Global(_)) {
                return GlobalEffects::Unknown;
            }
            pending.extend(crate::collect::child_blocks(statement));
            if crate::walk::statement_expressions(statement)
                .iter()
                .any(|value| expression(value) == GlobalEffects::Unknown)
            {
                return GlobalEffects::Unknown;
            }
        }
    }
    GlobalEffects::None
}

pub fn variable_name(expression: &Expression) -> Option<&str> {
    let mut expression = expression;
    while let ExpressionKind::Index {
        target,
        index: Some(index),
    } = &expression.kind
    {
        if matches!(&target.kind, ExpressionKind::Variable(Variable::Named(n)) if n == "GLOBALS")
            && let ExpressionKind::Literal(Literal::String(bytes)) = &index.kind
        {
            return std::str::from_utf8(bytes).ok();
        }
        expression = target;
    }
    None
}

pub fn is_globals(expression: &Expression) -> bool {
    matches!(&expression.kind, ExpressionKind::Variable(Variable::Named(name)) if name == "GLOBALS")
}

pub fn expression(value: &Expression) -> GlobalEffects {
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        if matches!(
            &value.kind,
            ExpressionKind::Call { .. }
                | ExpressionKind::MethodCall { .. }
                | ExpressionKind::StaticCall { .. }
                | ExpressionKind::New { .. }
                | ExpressionKind::Eval(_)
                | ExpressionKind::Include { .. }
        ) || is_globals(value)
        {
            return GlobalEffects::Unknown;
        }
        pending.extend(crate::walk::children(value));
    }
    GlobalEffects::None
}
