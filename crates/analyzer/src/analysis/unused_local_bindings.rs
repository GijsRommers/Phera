//! In `$unused = calculate();`, report a local variable that is never read.
//! In `foreach ($items as $key => $item)`, also report an unread key.

use super::*;
use std::collections::BTreeMap;

enum Binding {
    Assignment,
    ForeachKey,
}

impl Analyzer<'_> {
    pub(super) fn check_unused_local_bindings(
        &mut self,
        body: &[Statement],
        parameters: &[Parameter],
        captures: &[ClosureUse],
    ) {
        if self.level < 4 || !self.options.unused_variables {
            return;
        }
        let mut candidates = Vec::new();
        let mut statements: Vec<_> = body.iter().collect();
        let mut expressions = Vec::new();
        let mut mentions = BTreeMap::new();
        while !statements.is_empty() || !expressions.is_empty() {
            if let Some(statement) = statements.pop() {
                statements.extend(
                    crate::collect::child_blocks(statement)
                        .into_iter()
                        .flatten(),
                );
                expressions.extend(crate::walk::statement_expressions(statement));
                match &statement.kind {
                    StatementKind::Expression(expression) => {
                        if let ExpressionKind::Assign { target, .. } = &expression.kind {
                            candidates.push((Binding::Assignment, target.as_ref()));
                        }
                    }
                    StatementKind::Foreach { key, value, .. } => {
                        if let Some(key) = key {
                            expressions.push(key);
                            candidates.push((Binding::ForeachKey, key));
                        }
                        expressions.push(value);
                    }
                    StatementKind::Switch { cases, .. } => {
                        expressions.extend(cases.iter().filter_map(|case| case.condition.as_ref()));
                    }
                    StatementKind::Global(_)
                    | StatementKind::Static(_)
                    | StatementKind::Goto(_) => {
                        return;
                    }
                    _ => {}
                }
                continue;
            }
            let Some(expression) = expressions.pop() else {
                break;
            };
            match &expression.kind {
                ExpressionKind::Variable(Variable::Named(name)) => {
                    *mentions.entry(name.as_str()).or_insert(0usize) += 1;
                }
                ExpressionKind::Variable(Variable::Dynamic(_))
                | ExpressionKind::Include { .. }
                | ExpressionKind::Eval(_) => return,
                ExpressionKind::ArrowFunction(arrow) => expressions.push(&arrow.body),
                ExpressionKind::Closure(closure) => {
                    for capture in &closure.uses {
                        *mentions.entry(capture.name.as_str()).or_insert(0) += 1;
                    }
                }
                ExpressionKind::Call { callee, .. } if dynamic_variable_access(callee) => return,
                _ => {}
            }
            expressions.extend(crate::walk::children(expression));
        }
        for (binding, target) in candidates {
            let ExpressionKind::Variable(Variable::Named(name)) = &target.kind else {
                continue;
            };
            if name.starts_with('_')
                || name == "GLOBALS"
                || name == "this"
                || mentions.get(name.as_str()) != Some(&1)
                || parameters.iter().any(|parameter| parameter.name == *name)
                || captures.iter().any(|capture| capture.name == *name)
            {
                continue;
            }
            let (identifier, label) = match binding {
                Binding::Assignment => ("variable.unused", "Variable"),
                Binding::ForeachKey => ("foreach.unusedKey", "Foreach key variable"),
            };
            self.diagnose(
                identifier,
                target.span,
                format!("{label} ${name} is never read."),
            );
        }
    }
}

fn dynamic_variable_access(callee: &Expression) -> bool {
    let ExpressionKind::Name(name) = &callee.kind else {
        // An indirect call can dispatch to compact() or get_defined_vars().
        return true;
    };
    matches!(
        name.text
            .rsplit('\\')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "compact" | "extract" | "get_defined_vars"
    )
}
