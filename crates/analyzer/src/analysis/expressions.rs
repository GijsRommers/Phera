//! Expression facts: `new DateTime` has a known class; `$unknown()` stays unknown.
//! The explicit depth bound also protects generated left-deep expression chains.

use super::chain_value::ChainValue;
use super::*;

impl Analyzer<'_> {
    pub fn expression(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        self.chain_expression(expression, scope, quiet)
            .finish(scope)
    }

    pub(super) fn chain_expression(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ChainValue {
        if self.depth >= MAX_DEPTH {
            self.context.incomplete = true;
            scope.unknown_writes();
            return ChainValue::plain(ValueType::Mixed);
        }
        self.depth += 1;
        let mut value = match &expression.kind {
            ExpressionKind::MethodCall { .. } if self.level >= 8 => {
                self.method_chain(expression, scope, quiet)
            }
            ExpressionKind::Property { .. } if self.level >= 8 => {
                self.property_chain(expression, scope, quiet)
            }
            ExpressionKind::Index { .. } if self.level >= 8 => {
                self.offset_chain(expression, scope, quiet)
            }
            _ => ChainValue::plain(self.expression_kind(expression, scope, quiet)),
        };
        self.depth -= 1;
        if self.level >= 2
            && !scope.facts.is_empty()
            && value.ty != ValueType::Never
            && !matches!(&expression.kind, ExpressionKind::Variable(Variable::Named(name)) if name != "this")
            && let Some(place) = crate::scope::Place::of(expression)
            && let Some(fact) = scope.facts.get(&place)
        {
            value.ty = fact.ty.clone();
        }
        value
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
        operation: &Expression,
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
        self.mixed_binary(operator, &left_ty, &right_ty, operation.span, false);
        if matches!(
            operator,
            BinaryOperator::Divide | BinaryOperator::Modulo | BinaryOperator::Power
        ) {
            self.record_exception(scope);
        }
        if self.level >= 4
            && let Some(ty) = self.constant_comparison(
                operator,
                (left, &left_ty),
                (right, &right_ty),
                scope,
                operation.span,
            )
        {
            return ty;
        }
        if self.level >= 4 && operator == BinaryOperator::Xor {
            let left_truth = dead_code::truth(&left_ty);
            let right_truth = dead_code::truth(&right_ty);
            self.constant_logical_operand(operation, left, left_truth, dead_code::Side::Left);
            self.constant_logical_operand(operation, right, right_truth, dead_code::Side::Right);
            if let (Some(left), Some(right)) = (left_truth, right_truth) {
                return ValueType::BoolLiteral(left != right);
            }
        }
        if self.level >= 3 {
            if matches!(
                operator,
                BinaryOperator::Subtract | BinaryOperator::Multiply | BinaryOperator::Divide
            ) && (self.strict_mixed(&left_ty) || self.strict_mixed(&right_ty))
            {
                return ValueType::OverflowingInt;
            }
            return super::operators::binary_type(operator, &left_ty, &right_ty);
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
        self.constant_condition(
            "ternary",
            "Ternary operator condition",
            condition,
            &branches,
        );
        let positive = if self.level >= 7 && then.is_none() {
            super::truthy_values::truthy_type(&branches.ty)
        } else {
            branches.ty
        };
        let (yes, yes_ty) = self.expression_branch(branches.yes, then, positive);
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
