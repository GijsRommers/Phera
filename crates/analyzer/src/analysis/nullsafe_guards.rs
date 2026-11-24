//! On the true path of `$request?->hasSession() === true`, the request exists.
//! The same fact follows if a nullsafe chain returns a non-null value.

use super::*;
use crate::scope::{AccessStep, Place};

impl Analyzer<'_> {
    /// On the equal path of `$label === $item->label`, a non-null label
    /// property also proves that the local label is non-null.
    pub(super) fn refine_non_null_comparison(
        &self,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
    ) {
        for (value, peer) in [(left, right), (right, left)] {
            if let Some(place) = Place::of(peer)
                && (place.members.is_empty()
                    || matches!(value.kind, ExpressionKind::Variable(Variable::Named(_)))
                        && place
                            .members
                            .iter()
                            .all(|step| matches!(step, AccessStep::Property(_))))
                && self.place_type(&place, scope).excludes_null()
            {
                self.refine_type(value, ValueType::Null, scope, false, value.span);
            }
        }
    }

    pub(super) fn remember_optional_result(
        &self,
        variable: &str,
        expression: &Expression,
        scope: &mut Scope,
    ) {
        if self.level < 8 || scope.dynamic || self.context.scope_kind == ScopeKind::Global {
            return;
        }
        let receiver = match &expression.kind {
            ExpressionKind::MethodCall {
                target,
                nullsafe: true,
                ..
            }
            | ExpressionKind::Property {
                target,
                nullsafe: true,
                ..
            } => match &target.kind {
                ExpressionKind::Variable(Variable::Named(receiver)) => Some(receiver.as_str()),
                _ => None,
            },
            _ => ternary_receiver(expression),
        };
        if let Some(receiver) = receiver
            && variable != receiver
            && !scope.aliases.contains_key(receiver)
            && !scope.external_references.contains(receiver)
            && !scope.aliases.contains_key(variable)
            && !scope.external_references.contains(variable)
        {
            scope.facts.remember_nullsafe_result(variable, receiver);
        }
    }

    pub(super) fn refine_nullsafe_receivers(&self, mut expression: &Expression, scope: &mut Scope) {
        if self.level < 8 || scope.dynamic {
            return;
        }
        loop {
            match &expression.kind {
                ExpressionKind::MethodCall {
                    target, nullsafe, ..
                }
                | ExpressionKind::Property {
                    target, nullsafe, ..
                } => {
                    if *nullsafe
                        && let Some(place) = Place::of(target)
                        && !scope.external_references.contains(&place.root)
                        && !scope.aliases.contains_key(&place.root)
                    {
                        let original = self.place_type(&place, scope);
                        let non_null = original.without_null();
                        if non_null != original && non_null != ValueType::Never {
                            scope.facts.record(place, non_null, None);
                        }
                    }
                    expression = target;
                }
                ExpressionKind::Index { target, .. } => expression = target,
                _ => break,
            }
        }
    }
}

// A non-null result of `$item === null ? null : $item->name()` proves $item exists.
fn ternary_receiver(expression: &Expression) -> Option<&str> {
    let ExpressionKind::Ternary {
        condition,
        then: Some(then),
        otherwise,
    } = &expression.kind
    else {
        return None;
    };
    let ExpressionKind::Binary {
        operator,
        left,
        right,
    } = &condition.kind
    else {
        return None;
    };
    let (skipped, evaluated) = match operator {
        BinaryOperator::Identical => (then, otherwise),
        BinaryOperator::NotIdentical => (otherwise, then),
        _ => return None,
    };
    if !matches!(skipped.kind, ExpressionKind::Literal(Literal::Null))
        || Place::of(evaluated).is_none() && !matches!(evaluated.kind, ExpressionKind::Literal(_))
    {
        return None;
    }
    match (&left.kind, &right.kind) {
        (
            ExpressionKind::Variable(Variable::Named(receiver)),
            ExpressionKind::Literal(Literal::Null),
        )
        | (
            ExpressionKind::Literal(Literal::Null),
            ExpressionKind::Variable(Variable::Named(receiver)),
        ) => Some(receiver),
        _ => None,
    }
}
