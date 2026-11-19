//! A bare `return;` in `function f(): Generator { yield 1; return; }` ends iteration.
//! The native return type describes the generator object created by calling f().

use phera_ast::{ExpressionKind, Statement};

pub fn body_yields(body: &[Statement]) -> bool {
    let mut blocks = vec![body];
    while let Some(block) = blocks.pop() {
        for statement in block {
            blocks.extend(crate::collect::child_blocks(statement));
            let mut pending = crate::walk::statement_expressions(statement);
            while let Some(expression) = pending.pop() {
                if matches!(
                    expression.kind,
                    ExpressionKind::Yield { .. } | ExpressionKind::YieldFrom(_)
                ) {
                    return true;
                }
                // The walker does not enter a nested closure or function body.
                pending.extend(crate::walk::children(expression));
            }
        }
    }
    false
}
