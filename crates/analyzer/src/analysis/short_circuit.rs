//! Coalescing probes safely: `$x ?? throw new Exception` defines `$x` on continuation.
//! Only expression branches that complete contribute facts after the guard.

use super::*;

impl Analyzer<'_> {
    pub fn coalesce(
        &mut self,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        self.redundant_null_coalesce(left, right, scope, "??");
        self.constant_null_check(left, scope, dead_code::NullCheck::Coalesce);
        let nullish = self.nullish(left, scope);
        let left_ty = self.expression(left, scope, true);
        self.check_nullsafe_coalesce(left, &left_ty);
        if left_ty == ValueType::Never || nullish == Some(false) {
            return left_ty;
        }
        if nullish == Some(true) {
            return self.expression(right, scope, quiet);
        }
        let mut skipped = scope.clone();
        self.refine_defined(left, &mut skipped);
        let right_ty = self.expression(right, scope, quiet);
        *scope = if right_ty == ValueType::Never {
            skipped
        } else {
            Scope::join(&[skipped, std::mem::take(scope)])
        };
        left_ty.without_null().join(&right_ty)
    }

    pub fn nullish(&self, expression: &Expression, scope: &Scope) -> Option<bool> {
        if let ExpressionKind::Literal(value) = &expression.kind {
            return Some(matches!(value, Literal::Null));
        }
        if let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind {
            let variable = scope.get(name);
            if variable.definition == Definition::Absent && !scope.dynamic {
                return Some(true);
            }
            if variable.ty == ValueType::Null {
                return Some(true);
            }
            if variable.definition == Definition::Definite && variable.ty.excludes_null() {
                return Some(false);
            }
        }
        None
    }
}
