//! Conditional writes: `$x ??= new C` preserves `$x` when it is already non-null.
//! A throwing default contributes no continuing path to the resulting scope.

use super::*;

impl Analyzer<'_> {
    pub fn coalesce_assignment(
        &mut self,
        target: &Expression,
        value: &Expression,
        scope: &mut Scope,
    ) -> ValueType {
        self.redundant_null_coalesce(target, value, scope, "??=");
        let nullish = self.nullish(target, scope);
        let original = self.expression(target, scope, true);
        if original == ValueType::Never || nullish == Some(false) {
            return original;
        }
        let mut skipped = scope.clone();
        self.refine_defined(target, &mut skipped);
        let mut replacement = self.expression(value, scope, false);
        let mut paths = Vec::new();
        if nullish != Some(true) {
            paths.push(skipped);
        }
        if replacement != ValueType::Never
            && self.write(target, replacement.clone(), scope) == Evaluation::Completes
        {
            replacement = self.assignment_result(target, scope, replacement);
            paths.push(scope.clone());
        }
        if paths.is_empty() {
            return ValueType::Never;
        }
        *scope = Scope::join(&paths);
        let result = if nullish == Some(true) {
            replacement
        } else {
            original.without_null().join(&replacement)
        };
        if self.level >= 7
            && let Some(place) = crate::scope::Place::of(target)
        {
            scope.facts.record(place, result.clone(), None);
        }
        result
    }
}
