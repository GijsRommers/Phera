//! In `if ($node->child)`, the child is non-null inside the branch.
//! The successful operand of `$value ?: $fallback` uses the same refinement.

use super::*;
use crate::scope::Place;

pub(super) fn truthy_type(ty: &ValueType) -> ValueType {
    let variants = match ty {
        ValueType::Union(arms) => arms.as_ref(),
        ty => std::slice::from_ref(ty),
    };
    variants
        .iter()
        .filter(|arm| dead_code::truth(arm) != Some(false))
        .fold(ValueType::Never, |result, arm| {
            let arm = if *arm == ValueType::Bool {
                ValueType::BoolLiteral(true)
            } else {
                super::arrays::when_truthy(arm.clone())
            };
            result.join(&arm)
        })
}

impl Analyzer<'_> {
    pub fn refine_truthy_place(&self, expression: &Expression, scope: &mut Scope) {
        let Some(place) = Place::of(expression) else {
            return;
        };
        let original = self.place_type(&place, scope);
        let narrowed = truthy_type(&original);
        if narrowed != original && narrowed != ValueType::Never {
            scope.facts.record(place, narrowed, Some(expression.span));
        }
    }
}
