//! `$value = 1; foreach ($items as $value) {}` overwrites an existing variable.

use crate::analysis::*;
use crate::scope::Definition;

impl Analyzer<'_> {
    pub(in crate::analysis) fn foreach_overwrite(
        &mut self,
        key: Option<&Expression>,
        value: &Expression,
        scope: &Scope,
        span: Span,
    ) {
        if !self.options.strict {
            return;
        }
        if let Some(key) = key {
            self.foreach_overwritten_variable(key, scope, "key", span);
        }
        let mut pending = vec![value];
        while let Some(target) = pending.pop() {
            match &target.kind {
                ExpressionKind::Array(items) => {
                    pending.extend(items.iter().rev().map(|item| &item.value));
                }
                ExpressionKind::List(items) => {
                    pending.extend(items.iter().rev().flatten().map(|item| &item.value));
                }
                _ => self.foreach_overwritten_variable(target, scope, "value", span),
            }
        }
    }

    fn foreach_overwritten_variable(
        &mut self,
        target: &Expression,
        scope: &Scope,
        kind: &str,
        span: Span,
    ) {
        if let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
            && scope.get(name).definition == Definition::Definite
        {
            let id = if kind == "key" {
                "foreach.keyOverwrite"
            } else {
                "foreach.valueOverwrite"
            };
            self.diagnose(
                id,
                span,
                format!("Foreach overwrites ${name} with its {kind} variable."),
            );
        }
    }
}
