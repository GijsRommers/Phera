//! Strict rules keep `for ($i = 0; $maybe; $i++) {}` from guaranteeing
//! that a later `foreach ($list as $i)` overwrites an existing variable.

use crate::analysis::*;
use crate::{constants::ConstantValue, scope::GuardKey};

pub(in crate::analysis) fn foreach_definitions(before: &Scope, paths: &[Scope]) -> Option<Scope> {
    if before.dynamic
        || !before
            .values
            .values()
            .any(|value| *value == ConstantValue::EmptyArray)
    {
        return None;
    }
    Some(Scope::join(paths))
}

// Non-emptiness of an initially empty accumulator proves the loop ran.
pub(in crate::analysis) fn remember_foreach_definitions(
    scope: &mut Scope,
    before: &Scope,
    entered: &Scope,
) {
    for (name, value) in &before.values {
        if *value == ConstantValue::EmptyArray
            && entered.values.get(name) != Some(value)
            && !scope.aliases.contains_key(name)
            && !scope.external_references.contains(name)
            && scope.get(name).definition == Definition::Definite
        {
            scope.remember_definitions(GuardKey::NonEmptyArray(name.clone()), Some(entered), None);
        }
    }
}

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
