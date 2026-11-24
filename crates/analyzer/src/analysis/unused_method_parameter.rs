//! A private method such as `private function size(int $unused): int { return 1; }`
//! has an unused parameter when its body and PHPDoc contract never refer to it.

use super::*;
use std::collections::BTreeSet;

impl Analyzer<'_> {
    pub(super) fn unused_method_parameters(&mut self, method: &Method, doc: Option<Span>) {
        if self.level < 4
            || !self.options.unused_parameters
            || method.modifiers.visibility != Some(Visibility::Private)
            || method.name.name.starts_with("__")
            || method.parameters.is_empty()
        {
            return;
        }
        let Some(body) = method.body.as_deref().filter(|body| !body.is_empty()) else {
            return;
        };
        // These annotations can use a parameter at the call site even
        // when the method body does not read it.
        if crate::phpdoc::text(&self.file.source, doc)
            .is_some_and(|doc| doc.contains("-assert") || doc.contains(" is "))
        {
            return;
        }
        let Some(mentioned) = mentioned_variables(body) else {
            return;
        };
        let Some(class) = &self.context.class else {
            return;
        };
        let method_name = format!("{class}::{}()", method.name.name);
        for parameter in &method.parameters {
            if !parameter.is_promoted()
                && !parameter.name.starts_with('_')
                && !mentioned.contains(parameter.name.as_str())
            {
                self.diagnose(
                    "method.unusedParameter",
                    parameter.span,
                    format!(
                        "Method {method_name} has an unused parameter ${}.",
                        parameter.name
                    ),
                );
            }
        }
    }
}

fn mentioned_variables(body: &[Statement]) -> Option<BTreeSet<&str>> {
    let mut statements: Vec<_> = body.iter().collect();
    let mut expressions = Vec::new();
    let mut names = BTreeSet::new();
    while !statements.is_empty() || !expressions.is_empty() {
        if let Some(statement) = statements.pop() {
            statements.extend(
                crate::collect::child_blocks(statement)
                    .into_iter()
                    .flatten(),
            );
            expressions.extend(crate::walk::statement_expressions(statement));
            if let StatementKind::Switch { cases, .. } = &statement.kind {
                expressions.extend(cases.iter().filter_map(|case| case.condition.as_ref()));
            }
            continue;
        }
        let expression = expressions.pop()?;
        match &expression.kind {
            ExpressionKind::Variable(Variable::Named(name)) => {
                names.insert(name.as_str());
            }
            ExpressionKind::Variable(Variable::Dynamic(_))
            | ExpressionKind::Include { .. }
            | ExpressionKind::Eval(_) => return None,
            ExpressionKind::ArrowFunction(arrow) => expressions.push(&arrow.body),
            ExpressionKind::Closure(closure) => {
                names.extend(closure.uses.iter().map(|capture| capture.name.as_str()));
            }
            ExpressionKind::Call { callee, .. }
                if matches!(&callee.kind, ExpressionKind::Name(name)
                if matches!(
                    name.text.rsplit('\\').next().unwrap_or_default().to_ascii_lowercase().as_str(),
                    "compact" | "get_defined_vars" | "func_get_args" | "func_get_arg"
                    | "func_num_args" | "extract"
                )) =>
            {
                return None;
            }
            _ => {}
        }
        expressions.extend(crate::walk::children(expression));
    }
    Some(names)
}
