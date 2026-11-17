//! Lvalue evaluation: `[0 => $key, $key => $value] = [1, 2]` binds left to right.
//! Keys and receiver expressions may throw, so writes expose their completion state.

use super::*;

enum Binding<'a> {
    Target(&'a Expression, ValueType),
    Key(&'a Expression),
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
                Binding::Key(key) => {
                    if self.expression(key, scope, false) == ValueType::Never {
                        return Evaluation::Never;
                    }
                    continue;
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
                    for item in items.iter().rev().flatten() {
                        if item.by_reference {
                            self.external_reference(&item.value, scope);
                        }
                        pending.push(Binding::Target(&item.value, ValueType::Mixed));
                        if let Some(key) = &item.key {
                            pending.push(Binding::Key(key));
                        }
                    }
                }
                ExpressionKind::Array(items) => {
                    for item in items.iter().rev() {
                        if item.by_reference {
                            self.external_reference(&item.value, scope);
                        }
                        pending.push(Binding::Target(&item.value, ValueType::Mixed));
                        if let Some(key) = &item.key {
                            pending.push(Binding::Key(key));
                        }
                    }
                }
                ExpressionKind::Index { .. } => {
                    if self.write_index(target, scope) == Evaluation::Never {
                        return Evaluation::Never;
                    }
                }
                _ => {
                    if self.expression(target, scope, false) == ValueType::Never {
                        return Evaluation::Never;
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
        for key in keys.into_iter().rev() {
            if self.expression(key, scope, false) == ValueType::Never {
                return Evaluation::Never;
            }
        }
        if let Some((name, old)) = variable {
            let ty = if old.definition == Definition::Absent {
                ValueType::Array
            } else if old.ty == scope.get(name).ty {
                old.ty
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
