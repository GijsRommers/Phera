//! If `($count ?? 0) > 0` succeeds, the count could not have been null.

use super::*;
use crate::constants::{ConstantValue, value};

impl Analyzer<'_> {
    pub(super) fn refine_coalesced_comparison(
        &self,
        operator: BinaryOperator,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        truth: bool,
    ) {
        let (target, when_null, other) = if let Some((target, fallback)) = coalesced(left) {
            (target, integer(fallback, scope), integer(right, scope))
        } else if let Some((target, fallback)) = coalesced(right) {
            (target, integer(left, scope), integer(fallback, scope))
        } else {
            return;
        };
        let Some((left, right)) = when_null.zip(other) else {
            return;
        };
        let matches = match operator {
            BinaryOperator::Less => left < right,
            BinaryOperator::LessEqual => left <= right,
            BinaryOperator::Greater => left > right,
            BinaryOperator::GreaterEqual => left >= right,
            _ => return,
        };
        if matches != truth {
            self.refine_type(target, ValueType::Null, scope, false, target.span);
        }
    }
}

fn coalesced(expression: &Expression) -> Option<(&Expression, &Expression)> {
    match &expression.kind {
        ExpressionKind::Binary {
            operator: BinaryOperator::NullCoalesce,
            left,
            right,
        } => Some((left, right)),
        _ => None,
    }
}

fn integer(expression: &Expression, scope: &Scope) -> Option<i64> {
    match value(expression, scope)? {
        ConstantValue::Int(integer) => Some(integer),
        _ => None,
    }
}
