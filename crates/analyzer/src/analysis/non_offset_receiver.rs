//! `$matches[0]` can fail if `$matches` is a list or false.
//! Quiet reads such as `$matches[0] ?? ''` can handle false.

use super::*;

impl Analyzer<'_> {
    pub(super) fn check_offset_receiver(
        &mut self,
        receiver: &ValueType,
        key: &ValueType,
        span: Span,
    ) {
        if self.level < 7 {
            return;
        }
        let ValueType::Union(arms) = receiver else {
            return;
        };
        if arms.contains(&ValueType::BoolLiteral(false))
            && arms.iter().any(super::operators::is_array)
        {
            self.diagnose(
                "offsetAccess.nonOffsetAccessible",
                span,
                format!("Cannot access offset {key} on {receiver}."),
            );
        }
    }

    pub(super) fn refine_nonnull_offset_receiver(
        &self,
        expression: &Expression,
        scope: &mut Scope,
    ) {
        if self.level < 7 {
            return;
        }
        let ExpressionKind::Index { target, .. } = &expression.kind else {
            return;
        };
        let ExpressionKind::Variable(Variable::Named(name)) = &target.kind else {
            return;
        };
        let Some(place) = crate::scope::Place::of(target) else {
            return;
        };
        let ValueType::Union(arms) = self.place_type(&place, scope) else {
            return;
        };
        if !arms.contains(&ValueType::BoolLiteral(false))
            || !arms.iter().any(super::operators::is_array)
        {
            return;
        }
        let ty = arms
            .iter()
            .filter(|ty| **ty != ValueType::BoolLiteral(false))
            .fold(ValueType::Never, |left, right| left.join(right));
        if let Some(variable) = scope.variables.get_mut(name) {
            variable.ty = ty.clone();
        }
        scope.facts.record(place, ty, None);
    }
}
