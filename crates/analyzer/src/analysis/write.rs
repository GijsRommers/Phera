//! Lvalue evaluation: `[0 => $key, $key => $value] = [1, 2]` binds left to right.
//! Keys and receiver expressions may throw, so writes expose their completion state.

use super::*;

enum Binding<'a> {
    Target(&'a Expression, ValueType),
    Element(&'a ArrayItem, usize, ValueType),
}

impl Analyzer<'_> {
    pub fn write(&mut self, target: &Expression, ty: ValueType, scope: &mut Scope) -> Evaluation {
        if self.depth >= MAX_DEPTH {
            self.context.incomplete = true;
            scope.unknown_writes();
            return Evaluation::Completes;
        }
        self.depth += 1;
        let result = self.write_targets(target, ty, scope);
        self.depth -= 1;
        result
    }

    fn write_targets(
        &mut self,
        target: &Expression,
        ty: ValueType,
        scope: &mut Scope,
    ) -> Evaluation {
        let mut pending = vec![Binding::Target(target, ty)];
        while let Some(binding) = pending.pop() {
            let (target, ty) = match binding {
                Binding::Element(item, index, array) => {
                    let key = item
                        .key
                        .as_ref()
                        .map_or(ValueType::IntLiteral(index as i64), |key| {
                            self.expression(key, scope, false)
                        });
                    if key == ValueType::Never {
                        return Evaluation::Never;
                    }
                    let ty = if self.level >= 2 && !item.by_reference {
                        super::arrays::offset_type(&array, &key)
                    } else {
                        ValueType::Mixed
                    };
                    (&item.value, ty)
                }
                Binding::Target(target, ty) => (target, ty),
            };
            match &target.kind {
                ExpressionKind::Variable(Variable::Named(name)) => scope.define(name, ty),
                ExpressionKind::Variable(Variable::Dynamic(name)) => {
                    if self.expression(name, scope, false) == ValueType::Never {
                        return Evaluation::Never;
                    }
                    scope.unknown_writes();
                }
                ExpressionKind::List(items) => {
                    for (index, item) in items
                        .iter()
                        .enumerate()
                        .rev()
                        .filter_map(|(index, item)| item.as_ref().map(|item| (index, item)))
                    {
                        if item.by_reference {
                            self.external_reference(&item.value, scope);
                        }
                        pending.push(Binding::Element(item, index, ty.clone()));
                    }
                }
                ExpressionKind::Array(items) => {
                    for (index, item) in items.iter().enumerate().rev() {
                        if item.by_reference {
                            self.external_reference(&item.value, scope);
                        }
                        pending.push(Binding::Element(item, index, ty.clone()));
                    }
                }
                ExpressionKind::Index { .. } => {
                    if self.write_index(target, &ty, scope) == Evaluation::Never {
                        return Evaluation::Never;
                    }
                }
                _ => {
                    if self.write_member(target, &ty, scope) == ValueType::Never {
                        return Evaluation::Never;
                    }
                }
            }
        }
        Evaluation::Completes
    }

    fn write_member(
        &mut self,
        target: &Expression,
        ty: &ValueType,
        scope: &mut Scope,
    ) -> ValueType {
        let property = matches!(
            target.kind,
            ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. }
        );
        let expected = if self.level >= 3 && property {
            self.member_expressions(target, scope, false)
        } else {
            self.expression(target, scope, false)
        };
        if expected == ValueType::Never {
            return expected;
        }
        let place = if self.level >= 2 && property {
            crate::scope::Place::of(target)
        } else {
            None
        };
        if property {
            self.check_property_assignment(target, place.as_ref(), &expected, ty, scope);
        }
        if self.level >= 2 {
            let stored = place
                .as_ref()
                .and_then(|place| self.stored_property_value(place, ty, scope));
            scope.forget_member_write(target);
            if (matches!(target.kind, ExpressionKind::Property { .. })
                || self.level >= 7 && matches!(target.kind, ExpressionKind::StaticProperty { .. }))
                && let Some(place) = place
            {
                scope
                    .facts
                    .record(place, stored.unwrap_or_else(|| ty.clone()), None);
            }
        }
        expected
    }

    fn write_index(
        &mut self,
        target: &Expression,
        value: &ValueType,
        scope: &mut Scope,
    ) -> Evaluation {
        let mut base = target;
        let mut keys = Vec::new();
        while let ExpressionKind::Index { target, index } = &base.kind {
            keys.push((
                base.span,
                index.as_deref(),
                self.level >= 9 && scope.facts.has_expression(base),
            ));
            base = target;
        }
        let mut receiver = ValueType::Mixed;
        let variable = if let ExpressionKind::Variable(Variable::Named(name)) = &base.kind {
            let old = scope.get(name);
            if old.definition == Definition::Absent {
                scope.variables.insert(
                    name.clone(),
                    crate::scope::VariableInfo {
                        definition: Definition::Unknown,
                        ty: ValueType::Mixed,
                    },
                );
            }
            if self.level >= 9 {
                receiver = old.ty.clone();
            }
            Some((name, old))
        } else {
            if self.level >= 9
                && matches!(
                    base.kind,
                    ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. }
                )
            {
                receiver = self.write_member(base, &ValueType::Mixed, scope);
                if receiver == ValueType::Never {
                    return Evaluation::Never;
                }
            } else if self.write(base, ValueType::Mixed, scope) == Evaluation::Never {
                return Evaluation::Never;
            }
            None
        };
        for (span, key, specified) in keys.into_iter().rev() {
            let key = key.map(|key| self.expression(key, scope, false));
            if key == Some(ValueType::Never) {
                return Evaluation::Never;
            }
            if !specified {
                self.mixed_offset(&receiver, key.as_ref(), span);
            }
        }
        if let Some((name, old)) = variable {
            let ty = if old.definition == Definition::Absent {
                ValueType::Array
            } else if old.ty == scope.get(name).ty {
                match old.ty {
                    ValueType::Shape(_) | ValueType::Collection(_) => ValueType::Array,
                    ValueType::StringLiteral(_) | ValueType::StringRefinement(_) => {
                        ValueType::String
                    }
                    ty => ty,
                }
            } else {
                ValueType::Mixed
            };
            scope.define(name, ty);
        }
        if self.context.scope_kind == ScopeKind::Global
            && let Some(name) = crate::global_effects::variable_name(target)
        {
            scope.define(name, ValueType::Mixed);
        }
        if self.strict_mixed(&receiver)
            && let Some(place) = crate::scope::Place::of(target)
        {
            scope.facts.record(place, value.clone(), None);
        }
        Evaluation::Completes
    }
}
