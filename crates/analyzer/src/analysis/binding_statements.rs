//! Scope declarations: `global $x`, `static $x = 1`, and `unset($x)` change bindings.
//! Unsetting a reference detaches its name without unsetting the other aliases.

use super::*;

impl Analyzer<'_> {
    pub fn binding_statement(&mut self, statement: &Statement, scope: &mut Scope) {
        match &statement.kind {
            StatementKind::Global(variables) => {
                for variable in variables {
                    match variable {
                        Variable::Named(name) => {
                            scope.define(name, ValueType::Mixed);
                            scope.external_references.insert(name.clone());
                        }
                        Variable::Dynamic(e) => {
                            self.expression(e, scope, false);
                            scope.unknown_writes();
                        }
                    }
                }
            }
            StatementKind::Static(variables) => {
                for variable in variables {
                    let ty = variable
                        .default
                        .as_ref()
                        .map_or(ValueType::Mixed, |e| self.expression(e, scope, false));
                    // The initializer runs once. A later invocation can see
                    // a value written by an earlier one.
                    let ty = if self.level >= 4 {
                        ValueType::Mixed
                    } else {
                        ty
                    };
                    scope.define(&variable.name, ty);
                }
            }
            StatementKind::Unset(expressions) => {
                for expression in expressions {
                    self.unset(expression, scope);
                }
            }
            _ => {}
        }
    }

    fn unset(&mut self, expression: &Expression, scope: &mut Scope) {
        self.expression(expression, scope, true);
        if self.level >= 2 {
            if let Some(place) = crate::scope::Place::of(expression) {
                scope.facts.forget_variable(&place.root);
            }
            if matches!(&expression.kind, ExpressionKind::Property { .. }) {
                scope.forget_member_write(expression);
            }
        }
        let name = match &expression.kind {
            ExpressionKind::Variable(Variable::Named(name)) => Some(name.as_str()),
            ExpressionKind::Index { target, .. }
                if self.context.scope_kind == ScopeKind::Global
                    && crate::global_effects::is_globals(target) =>
            {
                crate::global_effects::variable_name(expression)
            }
            _ => None,
        };
        if let Some(name) = name {
            scope.variables.remove(name);
            scope.detach(name);
            scope.values.remove(name);
        }
    }
}
