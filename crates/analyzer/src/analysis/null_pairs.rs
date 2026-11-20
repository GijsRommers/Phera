//! After `if ($a === null && $b === null) { return; }`, finding
//! that `$a === null` proves that $b is non-null.

use super::*;
use crate::scope::Place;

impl Analyzer<'_> {
    pub(super) fn remember_non_null_pair(&self, condition: &Expression, scope: &mut Scope) {
        let ExpressionKind::Binary {
            operator: BinaryOperator::And,
            left,
            right,
        } = &condition.kind
        else {
            return;
        };
        let Some((left, right)) = null_test(left).zip(null_test(right)) else {
            return;
        };
        if left != right
            && !scope.dynamic
            && [left, right].into_iter().all(|name| {
                scope.get(name).definition == Definition::Definite
                    && !scope.external_references.contains(name)
                    && !scope.aliases.contains_key(name)
            })
        {
            scope.facts.remember_non_null_pair(left, right);
        }
    }

    pub(super) fn refine_other_non_null(&self, expression: &Expression, scope: &mut Scope) {
        let ExpressionKind::Variable(Variable::Named(name)) = &expression.kind else {
            return;
        };
        for other in scope.facts.known_non_null_when(name) {
            let ty = scope.get(&other).ty.without_null();
            if ty != ValueType::Never {
                scope.facts.record(Place::variable(&other), ty, None);
            }
        }
    }
}

fn null_test(expression: &Expression) -> Option<&str> {
    let ExpressionKind::Binary {
        operator: BinaryOperator::Identical,
        left,
        right,
    } = &expression.kind
    else {
        return None;
    };
    match (&left.kind, &right.kind) {
        (
            ExpressionKind::Variable(Variable::Named(name)),
            ExpressionKind::Literal(Literal::Null),
        )
        | (
            ExpressionKind::Literal(Literal::Null),
            ExpressionKind::Variable(Variable::Named(name)),
        ) => Some(name),
        _ => None,
    }
}
