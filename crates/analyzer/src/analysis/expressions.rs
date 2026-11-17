//! Expression facts: `new DateTime` has a known class; `$unknown()` stays unknown.
//! The explicit depth bound also protects generated left-deep expression chains.

use super::*;

impl Analyzer<'_> {
    pub fn expression(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        if self.depth >= MAX_DEPTH {
            self.context.incomplete = true;
            scope.unknown_writes();
            return ValueType::Mixed;
        }
        self.depth += 1;
        let ty = self.expression_kind(expression, scope, quiet);
        self.depth -= 1;
        ty
    }

    fn expression_kind(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Literal(_)
            | ExpressionKind::Variable(_)
            | ExpressionKind::Name(_)
            | ExpressionKind::MagicConstant(_) => self.atoms(expression, scope, quiet),
            ExpressionKind::Assign { .. }
            | ExpressionKind::AssignReference { .. }
            | ExpressionKind::CompoundAssign { .. }
            | ExpressionKind::IncDec { .. } => self.assignments(expression, scope, quiet),
            ExpressionKind::Call { .. }
            | ExpressionKind::New { .. }
            | ExpressionKind::MethodCall { .. }
            | ExpressionKind::StaticCall { .. } => self.invocations(expression, scope, quiet),
            ExpressionKind::Property { .. }
            | ExpressionKind::StaticProperty { .. }
            | ExpressionKind::ClassConstant { .. } => {
                self.member_expressions(expression, scope, quiet)
            }
            ExpressionKind::Isset(_)
            | ExpressionKind::Empty(_)
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Ternary { .. }
            | ExpressionKind::InstanceOf { .. } => self.logical(expression, scope, quiet),
            _ => self.composite(expression, scope, quiet),
        }
    }

    pub fn binary(
        &mut self,
        operator: BinaryOperator,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        let left_ty = self.expression(
            left,
            scope,
            quiet || operator == BinaryOperator::NullCoalesce,
        );
        if matches!(
            operator,
            BinaryOperator::And | BinaryOperator::Or | BinaryOperator::NullCoalesce
        ) {
            let before = scope.clone();
            self.refine(left, scope, operator != BinaryOperator::Or);
            let right_ty = self.expression(right, scope, quiet);
            *scope = Scope::join(&[before, scope.clone()]);
            return if operator == BinaryOperator::NullCoalesce {
                left_ty.join(&right_ty)
            } else {
                ValueType::Bool
            };
        }
        let right_ty = self.expression(right, scope, quiet);
        if matches!(
            operator,
            BinaryOperator::Divide | BinaryOperator::Modulo | BinaryOperator::Power
        ) {
            self.record_exception(scope);
        }
        match operator {
            BinaryOperator::Concat => ValueType::String,
            BinaryOperator::Equal
            | BinaryOperator::NotEqual
            | BinaryOperator::Identical
            | BinaryOperator::NotIdentical
            | BinaryOperator::Less
            | BinaryOperator::Greater
            | BinaryOperator::LessEqual
            | BinaryOperator::GreaterEqual => ValueType::Bool,
            _ => left_ty.join(&right_ty),
        }
    }

    pub fn ternary(
        &mut self,
        condition: &Expression,
        then: Option<&Expression>,
        otherwise: &Expression,
        scope: &mut Scope,
    ) -> ValueType {
        let condition_ty = self.expression(condition, scope, false);
        let mut left = scope.clone();
        let mut right = scope.clone();
        self.refine(condition, &mut left, true);
        self.refine(condition, &mut right, false);
        let left_ty = then.map_or(condition_ty, |e| self.expression(e, &mut left, false));
        let right_ty = self.expression(otherwise, &mut right, false);
        *scope = Scope::join(&[left, right]);
        left_ty.join(&right_ty)
    }
}
