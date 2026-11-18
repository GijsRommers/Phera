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
        let nullish = self.nullish(target, scope);
        let original = self.expression(target, scope, true);
        if original == ValueType::Never || nullish == Some(false) {
            return original;
        }
        let mut skipped = scope.clone();
        self.refine_defined(target, &mut skipped);
        let replacement = self.expression(value, scope, false);
        let mut paths = Vec::new();
        if nullish != Some(true) {
            paths.push(skipped);
        }
        if replacement != ValueType::Never
            && self.write(target, replacement.clone(), scope) == Evaluation::Completes
        {
            paths.push(scope.clone());
        }
        if paths.is_empty() {
            return ValueType::Never;
        }
        *scope = Scope::join(&paths);
        if nullish == Some(true) {
            replacement
        } else {
            original.without_null().join(&replacement)
        }
    }
}
