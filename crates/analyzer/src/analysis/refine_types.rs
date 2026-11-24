//! On the true path of `is_string($x)`, `$x` can be used as a string.
//! A false guard subtracts only union arms that are entirely covered by its test.

use super::*;
use crate::constant_types::literal as literal_type;
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
            self.refine_nonnull_offset_receiver(place, scope);
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
        self.refine_place(place, against, scope, truth, Some(span));
    }

    pub(super) fn refine_place(
        &self,
        place: Place,
        against: ValueType,
        scope: &mut Scope,
        truth: bool,
        span: Option<Span>,
    ) {
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
            } else {
                match &against {
                    ValueType::Union(tests) | ValueType::BenevolentUnion(tests) => {
                        tests.iter().fold(ValueType::Never, |types, test| {
                            types.join(&self.narrow_positive(arm, test))
                        })
                    }
                    test => self.narrow_positive(arm, test),
                }
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
                scope.facts.record(place, narrowed, span);
            }
        } else if self.strict_mixed(&original)
            && matches!(
                place.members.last(),
                Some(
                    AccessStep::IntegerOffset(_)
                        | AccessStep::StringOffset(_)
                        | AccessStep::VariableOffset(_)
                        | AccessStep::ArithmeticOffset(_)
                )
            )
            && scope.facts.get(&place).is_none()
        {
            scope.facts.record(place, original, None);
        }
    }

    // A positive test retains only the overlap with each tested alternative.
    fn narrow_positive(&self, arm: &ValueType, test: &ValueType) -> ValueType {
        use ValueType as T;
        let matches = test.accepts(arm, self.index, TypePolicy::Documentation);
        if matches == Compatibility::Accepts {
            arm.clone()
        } else if matches == Compatibility::Uncertain
            && matches!(
                (test, arm),
                (
                    T::UnknownCallable,
                    T::String | T::StringLiteral(_) | T::Class(_) | T::Generic(_) | T::Object
                ) | (T::Object, T::UnknownCallable)
            )
        {
            // Callable strings and invokable objects keep their original members.
            T::intersection(arm, test)
        } else if matches == Compatibility::Uncertain
            || arm.accepts(test, self.index, TypePolicy::Documentation) != Compatibility::Rejects
        {
            test.clone()
        } else if arm.class_name().is_some()
            && test.class_name().is_some()
            && !self.disjoint_classes(arm, test)
        {
            T::intersection(arm, test)
        } else {
            T::Never
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
                AccessStep::ClassName(kind, name) => {
                    let class = match name.as_str() {
                        "self" => self.context.class.clone(),
                        "parent" => self
                            .context
                            .class
                            .as_ref()
                            .and_then(|class| self.index.class(class))
                            .and_then(|class| class.parents.first())
                            .cloned(),
                        _ => Some(
                            self.context.names.resolve(
                                &Name {
                                    kind: *kind,
                                    text: name.clone(),
                                    span: Span::new(0, 0),
                                },
                                UseKind::Class,
                            )[0]
                            .clone(),
                        ),
                    };
                    ty = class.map_or(ValueType::Mixed, ValueType::Class);
                    continue;
                }
                AccessStep::IntegerOffset(key) => {
                    ty = self.offset_value(&ty, &ValueType::IntLiteral(*key));
                    continue;
                }
                AccessStep::StringOffset(key) => {
                    let key = ValueType::StringLiteral(key.as_slice().into());
                    ty = self.offset_value(&ty, &key);
                    continue;
                }
                AccessStep::VariableOffset(key) => {
                    ty = self.offset_value(&ty, &scope.get(key).ty);
                    continue;
                }
                AccessStep::ArithmeticOffset(_) => {
                    ty = self.offset_value(&ty, &ValueType::Mixed);
                    continue;
                }
                AccessStep::Property(name) | AccessStep::Method(name, _) => name,
            };
            ty = self.place_member_type(&ty, name, matches!(member, AccessStep::Property(_)));
        }
        ty
    }

    pub(super) fn place_member_type(
        &self,
        receiver: &ValueType,
        name: &str,
        property: bool,
    ) -> ValueType {
        if self.strict_mixed(receiver) {
            return ValueType::ImplicitMixed;
        }
        let arms = match receiver {
            ValueType::Union(arms) | ValueType::BenevolentUnion(arms) => arms.as_ref(),
            other => std::slice::from_ref(other),
        };
        let mut ty = ValueType::Never;
        for arm in arms {
            if arm == &ValueType::Null {
                continue;
            }
            let Some(class) = arm.class_name() else {
                return ValueType::Mixed;
            };
            let member = if property {
                self.index
                    .property(class, name)
                    .map(|property| property.effective_type(self.index))
                    .or_else(|| self.index.doc_property_type(class, name).cloned())
            } else {
                self.index
                    .method_owner(class, name)
                    .map(|(owner, signature)| {
                        signature
                            .clone()
                            .typed(self.index)
                            .for_receiver(self.index, owner, arm)
                            .returns
                    })
                    .or_else(|| {
                        self.index
                            .doc_method(class, name)
                            .map(|signature| signature.clone().typed(self.index).returns)
                    })
            };
            let Some(member) = member else {
                return ValueType::Mixed;
            };
            ty = ty.join(&self.relative_type(member, class));
        }
        if ty == ValueType::Never {
            ValueType::Mixed
        } else {
            ty
        }
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
            } else {
                self.refine_non_null_comparison(left, right, scope);
            }
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
