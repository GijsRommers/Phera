//! With weak typing, `$object->text = false` stores a string in a native
//! string property. A PHPDoc-only string property does not perform coercion.

use super::*;
use crate::{
    index::{ClassInfo, PropertyInfo},
    scope::{AccessStep, Place},
    types::{Compatibility, TypePolicy},
};

impl Analyzer<'_> {
    pub(super) fn assignment_result(
        &self,
        target: &Expression,
        scope: &Scope,
        value: ValueType,
    ) -> ValueType {
        if self.level >= 7
            && matches!(
                target.kind,
                ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. }
            )
            && let Some(place) = Place::of(target)
            && let Some(fact) = scope.facts.get(&place)
        {
            fact.ty.clone()
        } else {
            value
        }
    }

    pub(super) fn coerced_property_value(
        &self,
        place: &Place,
        value: &ValueType,
        scope: &Scope,
    ) -> Option<ValueType> {
        if self.level < 7 || self.strict_types {
            return None;
        }
        let native = &self.resolved_property(place, scope)?.1.ty;
        if matches!(
            native,
            ValueType::String | ValueType::Int | ValueType::Float | ValueType::Bool
        ) && native.accepts(value, self.index, TypePolicy::Documentation)
            != Compatibility::Accepts
            && native.accepts(value, self.index, TypePolicy::Coercive) == Compatibility::Accepts
        {
            Some(native.clone())
        } else {
            None
        }
    }

    pub(in crate::analysis) fn may_read_implicit_null(
        &self,
        expression: &Expression,
        scope: &Scope,
    ) -> bool {
        matches!(
            expression.kind,
            ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. }
        ) && Place::of(expression).is_some_and(|place| {
            scope.facts.get(&place).is_none()
                && self
                    .resolved_property(&place, scope)
                    .is_some_and(|(_, property)| {
                        property.default == crate::index::PropertyDefault::ImplicitNull
                    })
        })
    }

    pub(super) fn resolved_property(
        &self,
        place: &Place,
        scope: &Scope,
    ) -> Option<(&ClassInfo, &PropertyInfo)> {
        let mut receiver = place.clone();
        let AccessStep::Property(name) = receiver.members.pop()? else {
            return None;
        };
        let receiver = self.place_type(&receiver, scope);
        self.index
            .ancestry(receiver.class_name()?)
            .into_iter()
            .find_map(|base| base.properties.get(&name).map(|property| (base, property)))
    }
}
