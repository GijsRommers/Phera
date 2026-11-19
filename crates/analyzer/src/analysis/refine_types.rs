//! On the true path of `is_string($x)`, `$x` can be used as a string.
//! A false guard subtracts only union arms that are entirely covered by its test.

use super::*;
use crate::scope::{AccessStep, Place};
use crate::types::{Compatibility, TypePolicy};

impl Analyzer<'_> {
    pub fn refine_type(
        &self,
        place: &Expression,
        against: ValueType,
        scope: &mut Scope,
        truth: bool,
        span: Span,
    ) {
        let place = match &place.kind {
            ExpressionKind::Assign { target, .. } => target.as_ref(),
            _ => place,
        };
        let Some(place) = Place::of(place) else {
            return;
        };
        let original = self.place_type(&place, scope);
        let current = match &original {
            ValueType::Union(variants) => variants.as_ref(),
            other => std::slice::from_ref(other),
        };
        let narrowed = current.iter().fold(ValueType::Never, |out, arm| {
            let matches = against.accepts(arm, self.index, TypePolicy::Documentation);
            let ty = if !truth {
                if matches == Compatibility::Accepts {
                    ValueType::Never
                } else {
                    arm.clone()
                }
            } else if matches == Compatibility::Accepts {
                arm.clone()
            } else if matches == Compatibility::Uncertain
                || arm.accepts(&against, self.index, TypePolicy::Documentation)
                    == Compatibility::Accepts
            {
                against.clone()
            } else if let (Some(_), Some(_)) = (arm.class_name(), against.class_name()) {
                ValueType::intersection(arm, &against)
            } else {
                ValueType::Never
            };
            out.join(&ty)
        });
        if narrowed != original {
            // The flow pass at these levels visits both branches of unknown
            // conditions. Keep the positive type even if the input was disjoint.
            let narrowed = if narrowed == ValueType::Never && truth {
                against
            } else {
                narrowed
            };
            if narrowed != ValueType::Never {
                scope.facts.record(place, narrowed, Some(span));
            }
        }
    }

    fn place_type(&self, place: &Place, scope: &Scope) -> ValueType {
        if let Some(fact) = scope.facts.get(place) {
            return fact.ty.clone();
        }
        let mut ty = if place.root == "this" && self.context.object {
            self.context
                .this_class
                .as_ref()
                .map_or(ValueType::Mixed, |class| ValueType::Class(class.clone()))
        } else {
            scope.get(&place.root).ty
        };
        for member in &place.members {
            let Some(class) = ty.class_name() else {
                return ValueType::Mixed;
            };
            ty = match member {
                AccessStep::Property(member) => self
                    .index
                    .property(class, member)
                    .map(|property| property.effective_type(self.index)),
                AccessStep::Method(member) => self
                    .index
                    .method(class, member)
                    .or_else(|| self.index.doc_method(class, member))
                    .map(|signature| signature.clone().typed(self.index).returns),
            }
            .map_or(ValueType::Mixed, |ty| self.relative_type(ty, class));
        }
        ty
    }

    pub fn refine_identical(
        &self,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        truth: bool,
        span: Span,
    ) {
        if let Some(literal) = literal_type(right) {
            self.refine_type(left, literal, scope, truth, span);
        } else if let Some(literal) = literal_type(left) {
            self.refine_type(right, literal, scope, truth, span);
        }
    }

    pub fn refine_in_array(
        &self,
        arguments: &[Argument],
        scope: &mut Scope,
        truth: bool,
        span: Span,
    ) {
        let [subject, list, strict, ..] = arguments else {
            return;
        };
        if !matches!(
            strict.value.kind,
            ExpressionKind::Literal(Literal::Bool(true))
        ) {
            return;
        }
        let ExpressionKind::Array(items) = &list.value.kind else {
            return;
        };
        let mut possibilities = ValueType::Never;
        for item in items {
            let Some(literal) = literal_type(&item.value) else {
                return;
            };
            possibilities = possibilities.join(&literal);
        }
        if possibilities != ValueType::Never {
            self.refine_type(&subject.value, possibilities, scope, truth, span);
        }
    }
}

pub fn predicate_type(function: &str) -> Option<ValueType> {
    Some(match function {
        "is_null" => ValueType::Null,
        "is_bool" => ValueType::Bool,
        "is_int" | "is_integer" | "is_long" => ValueType::Int,
        "is_float" | "is_double" | "is_real" => ValueType::Float,
        "is_string" => ValueType::String,
        "is_array" => ValueType::Array,
        "is_object" => ValueType::Object,
        "is_resource" => ValueType::Resource,
        "is_iterable" => ValueType::Iterable,
        "is_callable" => ValueType::UnknownCallable,
        _ => return None,
    })
}

fn literal_type(expression: &Expression) -> Option<ValueType> {
    Some(match &expression.kind {
        ExpressionKind::Literal(Literal::Null) => ValueType::Null,
        ExpressionKind::Literal(Literal::Int(value)) => ValueType::IntLiteral(*value),
        ExpressionKind::Literal(Literal::Bool(value)) => ValueType::BoolLiteral(*value),
        ExpressionKind::Literal(Literal::String(value)) => {
            ValueType::StringLiteral(value.as_slice().into())
        }
        _ => return None,
    })
}
