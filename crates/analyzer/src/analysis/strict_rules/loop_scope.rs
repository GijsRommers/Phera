//! Strict rules keep `for ($i = 0; $maybe; $i++) {}` from guaranteeing
//! that a later `foreach ($list as $i)` overwrites an existing variable.

use crate::analysis::*;

impl Analyzer<'_> {
    pub(in crate::analysis) fn loop_initializer_locals<'a>(
        &self,
        initializers: &'a [Expression],
        scope: &Scope,
    ) -> Vec<&'a str> {
        if !self.options.strict {
            return Vec::new();
        }
        initializers
            .iter()
            .filter_map(|expression| {
                let ExpressionKind::Assign { target, .. } = &expression.kind else {
                    return None;
                };
                let ExpressionKind::Variable(Variable::Named(name)) = &target.kind else {
                    return None;
                };
                (scope.get(name).definition != Definition::Definite).then_some(name.as_str())
            })
            .collect()
    }
}
