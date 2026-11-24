//! In `$name = first(); $name = second(); echo $name;`, the first value is overwritten.

use super::*;
use std::collections::BTreeSet;

impl Analyzer<'_> {
    pub(super) fn check_overwritten_assignments(
        &mut self,
        body: &[Statement],
        watched: &BTreeSet<&str>,
    ) {
        if watched.is_empty() {
            return;
        }
        let mut blocks = vec![(body, BTreeSet::new())];
        while let Some((block, mut overwritten)) = blocks.pop() {
            for statement in block.iter().rev() {
                match &statement.kind {
                    StatementKind::If { .. } | StatementKind::Block(_) => {
                        for branch in crate::collect::child_blocks(statement) {
                            blocks.push((branch, overwritten.clone()));
                        }
                        overwritten.clear();
                    }
                    StatementKind::Expression(expression) => {
                        let assignment = if let ExpressionKind::Assign { target, value } =
                            &expression.kind
                            && let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
                        {
                            Some((target.span, name.as_str(), value.as_ref()))
                        } else {
                            None
                        };
                        if let Some((span, name, value)) = assignment {
                            if overwritten.contains(name) {
                                self.diagnose(
                                    "assign.overwritten",
                                    span,
                                    format!("Value assigned to variable ${name} is never read before being overwritten."),
                                );
                            }
                            if watched.contains(name) {
                                overwritten.insert(name);
                            }
                            forget_reads(value, &mut overwritten);
                        } else {
                            forget_reads(expression, &mut overwritten);
                        }
                    }
                    StatementKind::Echo(items) => {
                        for item in items {
                            forget_reads(item, &mut overwritten);
                        }
                    }
                    _ => overwritten.clear(),
                }
            }
        }
    }
}

fn forget_reads(expression: &Expression, overwritten: &mut BTreeSet<&str>) {
    let mut expressions = vec![expression];
    while let Some(expression) = expressions.pop() {
        match &expression.kind {
            ExpressionKind::Variable(Variable::Named(name)) => {
                overwritten.remove(name.as_str());
            }
            ExpressionKind::Closure(closure) => {
                for capture in &closure.uses {
                    overwritten.remove(capture.name.as_str());
                }
            }
            ExpressionKind::ArrowFunction(arrow) => expressions.push(&arrow.body),
            ExpressionKind::Throw(_) | ExpressionKind::Exit(_) => {
                overwritten.clear();
                return;
            }
            _ => {}
        }
        expressions.extend(crate::walk::children(expression));
    }
}
