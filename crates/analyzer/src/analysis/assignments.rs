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
                if ty == ValueType::Never {
                    return ty;
                }
                if self.write(target, ty.clone(), scope) == Evaluation::Never {
                    return ValueType::Never;
                }
                if let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
                    && let Some(value) = constant
                {
                    scope.values.insert(name.clone(), value);
                }
                ty
            }
            ExpressionKind::AssignReference { target, value } => {
                self.reference_assignment(target, value, scope)
            }
            ExpressionKind::CompoundAssign {
                target,
                value,
                operator,
            } => {
                if *operator == BinaryOperator::NullCoalesce {
                    return self.coalesce_assignment(target, value, scope);
                }
                let previous = self.expression(target, scope, false);
                if previous == ValueType::Never {
                    return ValueType::Never;
                }
                let ty = self.expression(value, scope, false);
                if ty == ValueType::Never {
                    return ty;
                }
                let ty = if self.level >= 3 {
                    super::operators::binary_type(*operator, &previous, &ty)
                } else {
                    ty
                };
                if self.write(target, ty.clone(), scope) == Evaluation::Never {
                    return ValueType::Never;
                }
                ty
            }
            ExpressionKind::IncDec { target, .. } => {
                if self.expression(target, scope, quiet) == ValueType::Never {
                    return ValueType::Never;
                }
                if self.write(target, ValueType::Mixed, scope) == Evaluation::Never {
                    return ValueType::Never;
                }
                ValueType::Mixed
            }
            _ => ValueType::Mixed,
        }
    }

    fn reference_assignment(
        &mut self,
        target: &Expression,
        value: &Expression,
        scope: &mut Scope,
    ) -> ValueType {
        self.ensure_reference(value, scope);
        let ty = self.expression(value, scope, true);
        if let (
            ExpressionKind::Variable(Variable::Named(left)),
            ExpressionKind::Variable(Variable::Named(right)),
        ) = (&target.kind, &value.kind)
            && left != right
        {
            scope.detach(left);
        }
        if ty == ValueType::Never {
            return ty;
        }
        if self.write(target, ty.clone(), scope) == Evaluation::Never {
            return ValueType::Never;
        }
        if let (
            ExpressionKind::Variable(Variable::Named(left)),
            ExpressionKind::Variable(Variable::Named(right)),
        ) = (&target.kind, &value.kind)
        {
            scope.reference(left, right);
        } else {
            self.external_reference(target, scope);
        }
        ty
    }
}
