//! On the true path of `$request?->hasSession() === true`, the request exists.
//! The same fact follows if a nullsafe chain returns a non-null value.

use super::*;
use crate::scope::Place;

impl Analyzer<'_> {
    pub(super) fn remember_nullsafe_result(
        &self,
        variable: &str,
        expression: &Expression,
        scope: &mut Scope,
    ) {
        if self.level < 8 || scope.dynamic || self.context.scope_kind == ScopeKind::Global {
            return;
        }
        let target = match &expression.kind {
            ExpressionKind::MethodCall {
                target,
                nullsafe: true,
                ..
            }
            | ExpressionKind::Property {
                target,
                nullsafe: true,
                ..
            } => target,
            _ => return,
        };
        if let ExpressionKind::Variable(Variable::Named(receiver)) = &target.kind
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
