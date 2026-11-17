//! Assignments define variables: `$x = new C` records the resulting class type.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;

impl Analyzer<'_> {
    pub fn assignments(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Assign { target, value } => {
                let constant = crate::constants::value(value, scope);
                let ty = self.expression(value, scope, false);
                self.write(target, ty.clone(), scope);
                if let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
                    && let Some(value) = constant
                {
                    scope.values.insert(name.clone(), value);
                }
                ty
            }
            ExpressionKind::AssignReference { target, value } => {
                self.ensure_reference(value, scope);
                let ty = self.expression(value, scope, true);
                self.write(target, ty.clone(), scope);
                if let (
                    ExpressionKind::Variable(Variable::Named(left)),
                    ExpressionKind::Variable(Variable::Named(right)),
                ) = (&target.kind, &value.kind)
                {
                    scope.reference(left, right);
                }
                ty
            }
            ExpressionKind::CompoundAssign {
                target,
                value,
                operator,
            } => {
                self.expression(target, scope, *operator == BinaryOperator::NullCoalesce);
                let ty = self.expression(value, scope, false);
                self.write(target, ty.clone(), scope);
                ty
            }
            ExpressionKind::IncDec { target, .. } => {
                self.expression(target, scope, quiet);
                self.write(target, ValueType::Mixed, scope);
                ValueType::Mixed
            }
            _ => ValueType::Mixed,
        }
    }
}
