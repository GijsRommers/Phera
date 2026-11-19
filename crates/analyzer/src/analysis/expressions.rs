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
        if operator == BinaryOperator::NullCoalesce {
            return self.coalesce(left, right, scope, quiet);
        }
        let left_ty = self.expression(left, scope, quiet);
        if left_ty == ValueType::Never {
            return left_ty;
        }
        let right_ty = self.expression(right, scope, quiet);
        if right_ty == ValueType::Never {
            return right_ty;
        }
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
        let branches = self.condition(condition, std::mem::take(scope), false);
        let (yes, yes_ty) = self.expression_branch(branches.yes, then, branches.ty);
        let (no, no_ty) = self.expression_branch(branches.no, Some(otherwise), ValueType::Mixed);
        if let Some(after) = super::condition::Condition::merge(yes, no) {
            *scope = after;
        }
        yes_ty.join(&no_ty)
    }

    fn expression_branch(
        &mut self,
        scope: Option<Scope>,
        expression: Option<&Expression>,
        fallback: ValueType,
    ) -> (Option<Scope>, ValueType) {
        let Some(mut scope) = scope else {
            return (None, ValueType::Never);
        };
        let ty = expression.map_or(fallback, |e| self.expression(e, &mut scope, false));
        ((ty != ValueType::Never).then_some(scope), ty)
    }
}
