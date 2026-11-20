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
                    if self.write_index(target, scope) == Evaluation::Never {
                        return Evaluation::Never;
                    }
                }
                _ => {
                    let given_property = matches!(
                        target.kind,
                        ExpressionKind::Property { .. } | ExpressionKind::StaticProperty { .. }
                    );
                    let expected = if self.level >= 3 && given_property {
                        self.member_expressions(target, scope, false)
                    } else {
                        self.expression(target, scope, false)
                    };
                    if expected == ValueType::Never {
                        return Evaluation::Never;
                    }
                    if given_property {
                        self.check_property_assignment(target, &expected, &ty);
                    }
                    if self.level >= 2 {
                        scope.forget_member_write(target);
                        if (matches!(target.kind, ExpressionKind::Property { .. })
                            || self.level >= 7
                                && matches!(target.kind, ExpressionKind::StaticProperty { .. }))
                            && let Some(place) = crate::scope::Place::of(target)
                        {
                            scope.facts.record(place, ty, None);
                        }
                    }
                }
            }
        }
        Evaluation::Completes
    }

    fn write_index(&mut self, target: &Expression, scope: &mut Scope) -> Evaluation {
        let mut base = target;
        let mut keys = Vec::new();
        while let ExpressionKind::Index { target, index } = &base.kind {
            if let Some(index) = index {
                keys.push(index.as_ref());
            }
            base = target;
        }
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
            Some((name, old))
        } else {
            if self.write(base, ValueType::Mixed, scope) == Evaluation::Never {
                return Evaluation::Never;
            }
            None
        };
        for (depth, key) in keys.into_iter().rev().enumerate() {
            let key = self.expression(key, scope, false);
            if key == ValueType::Never {
                return Evaluation::Never;
            }
            if depth == 0
                && let Some((_, old)) = &variable
            {
                self.mixed_offset(&old.ty, Some(&key), target.span);
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
        Evaluation::Completes
    }
}
