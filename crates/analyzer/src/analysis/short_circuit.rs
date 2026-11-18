//! Guards can terminate: `isset($x) || throw new Exception` defines `$x` on continuation.
//! Only expression branches that complete contribute facts after the guard.

use super::*;

impl Analyzer<'_> {
    pub fn short_circuit(
        &mut self,
        operator: BinaryOperator,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        let truth = self.condition_truth(left, scope);
        let nullish = self.nullish(left, scope);
        let left_ty = self.expression(
            left,
            scope,
            quiet || operator == BinaryOperator::NullCoalesce,
        );
        if left_ty == ValueType::Never {
            return left_ty;
        }
        let take_right = match operator {
            BinaryOperator::And => truth,
            BinaryOperator::Or => truth.map(|v| !v),
            _ => nullish,
        };
        if take_right == Some(false) {
            return if operator == BinaryOperator::NullCoalesce {
                left_ty
            } else {
                ValueType::Bool
            };
        }
        let mut skipped = scope.clone();
        if operator == BinaryOperator::NullCoalesce {
            self.refine_defined(left, &mut skipped);
        } else {
            self.refine(left, &mut skipped, operator == BinaryOperator::Or);
        }
        self.refine(left, scope, operator == BinaryOperator::And);
        let right_ty = self.expression(right, scope, quiet);
        let mut paths = Vec::new();
        if take_right != Some(true) {
            paths.push(skipped);
        }
        if right_ty != ValueType::Never {
            paths.push(scope.clone());
        }
        if paths.is_empty() {
            return ValueType::Never;
        }
        *scope = Scope::join(&paths);
        if operator == BinaryOperator::NullCoalesce {
            if take_right == Some(true) {
                right_ty
            } else {
                left_ty.without_null().join(&right_ty)
            }
        } else {
            ValueType::Bool
        }
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
