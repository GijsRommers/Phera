//! Scope declarations: `global $x`, `static $x = 1`, and `unset($x)` change bindings.
//! Unsetting a reference detaches its name without unsetting the other aliases.

use super::*;

impl Analyzer<'_> {
    pub fn binding_statement(&mut self, statement: &Statement, scope: &mut Scope) {
        match &statement.kind {
            StatementKind::Global(variables) => {
                for variable in variables {
                    match variable {
                        Variable::Named(name) => scope.define(name, ValueType::Mixed),
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
                    scope.define(&variable.name, ty);
                }
            }
            StatementKind::Unset(expressions) => {
                for e in expressions {
                    self.expression(e, scope, true);
                    if let ExpressionKind::Variable(Variable::Named(name)) = &e.kind {
                        scope.variables.remove(name);
                        scope.aliases.remove(name);
                        scope.values.remove(name);
                        for aliases in scope.aliases.values_mut() {
                            aliases.remove(name);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
