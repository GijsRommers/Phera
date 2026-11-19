//! In `while ($i < 5) { ++$i; }`, the initial value of $i is only
//! valid for the first condition check. Kill facts for loop-carried writes.

use super::*;
use std::collections::BTreeSet;

impl Analyzer<'_> {
    pub fn widen_loop<'expression>(
        &self,
        body: &[Statement],
        repeated: impl IntoIterator<Item = &'expression Expression>,
        scope: &mut Scope,
    ) {
        if self.level < 4 {
            return;
        }
        let mut statements: Vec<_> = body.iter().collect();
        let mut expressions: Vec<_> = repeated.into_iter().collect();
        let mut written = BTreeSet::new();
        while !statements.is_empty() || !expressions.is_empty() {
            if let Some(statement) = statements.pop() {
                statements.extend(
                    crate::collect::child_blocks(statement)
                        .into_iter()
                        .flatten(),
                );
                expressions.extend(crate::walk::statement_expressions(statement));
                match &statement.kind {
                    StatementKind::Foreach { key, value, .. } => {
                        written.extend(key.iter().filter_map(root_written));
                        written.extend(root_written(value));
                    }
                    StatementKind::Unset(places) => {
                        written.extend(places.iter().filter_map(root_written))
                    }
                    StatementKind::Try { catches, .. } => {
                        written
                            .extend(catches.iter().filter_map(|catch| catch.variable.as_deref()));
                    }
                    _ => {}
                }
                continue;
            }
            if let Some(expression) = expressions.pop() {
                match &expression.kind {
                    ExpressionKind::Assign { target, .. }
                    | ExpressionKind::AssignReference { target, .. }
                    | ExpressionKind::CompoundAssign { target, .. }
                    | ExpressionKind::IncDec { target, .. } => written.extend(root_written(target)),
                    ExpressionKind::Closure(closure) => {
                        written.extend(
                            closure
                                .uses
                                .iter()
                                .filter(|capture| capture.by_reference)
                                .map(|capture| capture.name.as_str()),
                        );
                    }
                    _ => {}
                }
                expressions.extend(crate::walk::children(expression));
            }
        }
        for name in written {
            let definition = scope.get(name).definition;
            scope.define(name, ValueType::Mixed);
            if let Some(variable) = scope.variables.get_mut(name) {
                variable.definition = if definition == Definition::Absent {
                    Definition::Possible
                } else {
                    definition
                };
            }
        }
    }
}

fn root_written(mut expression: &Expression) -> Option<&str> {
    while let ExpressionKind::Index { target, .. } = &expression.kind {
        expression = target;
    }
    match &expression.kind {
        ExpressionKind::Variable(Variable::Named(name)) => Some(name),
        _ => None,
    }
}
