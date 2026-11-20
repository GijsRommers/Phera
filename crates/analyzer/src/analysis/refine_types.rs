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
        let non_null_test =
            against == ValueType::Null && !truth || truth && against.excludes_null();
        let place = match &place.kind {
            ExpressionKind::Assign { target, .. } => target.as_ref(),
            ExpressionKind::Binary {
                operator: BinaryOperator::NullCoalesce,
                left,
                right,
            } if self.level >= 7
                && non_null_test
                && matches!(right.kind, ExpressionKind::Literal(Literal::Null)) =>
            {
                left.as_ref()
            }
            _ => place,
        };
        if non_null_test {
            self.refine_nullsafe_receivers(place, scope);
            if self.level >= 8 {
                self.refine_other_non_null(place, scope, false);
            }
        }
        let Some(place) = Place::of(place) else {
            return;
        };
        if self.level < 7 && place.root == crate::scope::SELF_PROPERTIES {
            return;
        }
        let original = self.place_type(&place, scope);
        let current = match &original {
            ValueType::Union(variants) | ValueType::BenevolentUnion(variants) => variants.as_ref(),
            other => std::slice::from_ref(other),
        };
        let narrowed = current.iter().fold(ValueType::Never, |out, arm| {
            let matches = against.accepts(arm, self.index, TypePolicy::Documentation);
            let ty = if !truth {
                if matches == Compatibility::Accepts {
                    ValueType::Never
                } else if let (ValueType::Bool, ValueType::BoolLiteral(value)) = (arm, &against) {
                    ValueType::BoolLiteral(!value)
                } else {
                    arm.clone()
                }
            } else if matches == Compatibility::Accepts {
                arm.clone()
            } else if matches == Compatibility::Uncertain
                && matches!(
                    (&against, arm),
                    (
                        ValueType::UnknownCallable,
                        ValueType::String
                            | ValueType::StringLiteral(_)
                            | ValueType::Class(_)
                            | ValueType::Generic(_)
                            | ValueType::Object
                    ) | (ValueType::Object, ValueType::UnknownCallable)
                )
            {
                // A callable string still passes a string parameter, and an
                // invokable object still has its original class members.
                ValueType::intersection(arm, &against)
            } else if matches == Compatibility::Uncertain
                || arm.accepts(&against, self.index, TypePolicy::Documentation)
                    != Compatibility::Rejects
            {
                against.clone()
            } else if let (Some(_), Some(_)) = (arm.class_name(), against.class_name()) {
                if self.disjoint_classes(arm, &against) {
                    ValueType::Never
                } else {
                    ValueType::intersection(arm, &against)
                }
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

    // The subtype checks have already ruled out either direction. PHP's
    // single inheritance makes an intersection of unrelated classes empty.
    fn disjoint_classes(&self, left: &ValueType, right: &ValueType) -> bool {
        [left, right].iter().all(|ty| {
            ty.class_name().is_some_and(|name| {
                self.index.class(name).is_some_and(|class| {
                    matches!(class.kind, ClassKind::Class | ClassKind::Enum)
                        && self.index.complete(name)
                })
            })
        })
    }

    pub(super) fn place_type(&self, place: &Place, scope: &Scope) -> ValueType {
        if let Some(fact) = scope.facts.get(place) {
            return fact.ty.clone();
        }
        let mut ty = if place.root == crate::scope::SELF_PROPERTIES {
            self.context
                .class
                .clone()
                .map_or(ValueType::Mixed, ValueType::Class)
        } else if place.root == "this" && self.context.object {
            self.context
                .this_class
                .as_ref()
                .map_or(ValueType::Mixed, |class| ValueType::Class(class.clone()))
        } else {
            scope.get(&place.root).ty
        };
        for member in &place.members {
            let name = match member {
                AccessStep::IntegerOffset(key) => {
                    ty = super::arrays::offset_type(&ty, &ValueType::IntLiteral(*key));
                    continue;
                }
                AccessStep::StringOffset(key) => {
                    let key = ValueType::StringLiteral(key.as_slice().into());
                    ty = super::arrays::offset_type(&ty, &key);
                    continue;
                }
                AccessStep::VariableOffset(key) => {
                    ty = super::arrays::offset_type(&ty, &scope.get(key).ty);
                    continue;
                }
                AccessStep::Property(name) | AccessStep::Method(name, _) => name,
            };
            let Some(class) = ty.class_name() else {
                return ValueType::Mixed;
            };
            ty = if matches!(member, AccessStep::Property(_)) {
                self.index
                    .property(class, name)
                    .map(|property| property.effective_type(self.index))
            } else {
                self.index
                    .method(class, name)
                    .or_else(|| self.index.doc_method(class, name))
                    .map(|signature| signature.clone().typed(self.index).returns)
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
            if self.level >= 8 && truth && literal == ValueType::Null {
                self.refine_other_non_null(left, scope, true);
            }
            self.refine_type(left, literal, scope, truth, span);
        } else if let Some(literal) = literal_type(left) {
            if self.level >= 8 && truth && literal == ValueType::Null {
                self.refine_other_non_null(right, scope, true);
            }
            self.refine_type(right, literal, scope, truth, span);
        } else if self.level >= 8 && truth {
            if let Some(class) = self.enum_case_class(right) {
                self.refine_type(left, ValueType::Class(class), scope, true, span);
            } else if let Some(class) = self.enum_case_class(left) {
                self.refine_type(right, ValueType::Class(class), scope, true, span);
            }
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
