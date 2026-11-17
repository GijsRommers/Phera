//! Remaining expression effects: closures capture scopes and throw terminates a path.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;

impl Analyzer<'_> {
    pub fn composite(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Array(items) => {
                for item in items {
                    if let Some(key) = &item.key
                        && self.expression(key, scope, false) == ValueType::Never
                    {
                        return ValueType::Never;
                    }
                    if item.by_reference {
                        self.ensure_reference(&item.value, scope);
                    }
                    if self.expression(&item.value, scope, false) == ValueType::Never {
                        return ValueType::Never;
                    }
                }
                ValueType::Array
            }
            ExpressionKind::Match(expression) => self.match_expression(expression, scope),
            ExpressionKind::Closure(closure) => self.closure(closure, scope, expression.span),
            ExpressionKind::ArrowFunction(arrow) => self.arrow(arrow, scope, expression.span),
            ExpressionKind::Throw(value) => {
                if self.expression(value, scope, false) == ValueType::Never {
                    return ValueType::Never;
                }
                self.record_exception(scope);
                ValueType::Never
            }
            ExpressionKind::Exit(value) => {
                if let Some(value) = value {
                    self.expression(value, scope, false);
                }
                ValueType::Never
            }
            ExpressionKind::Include { path, .. } | ExpressionKind::Eval(path) => {
                if self.expression(path, scope, false) == ValueType::Never {
                    return ValueType::Never;
                }
                self.record_exception(scope);
                scope.unknown_writes();
                ValueType::Mixed
            }
            _ => {
                for child in crate::walk::children(expression) {
                    if self.expression(child, scope, quiet) == ValueType::Never {
                        return ValueType::Never;
                    }
                }
                match expression.kind {
                    ExpressionKind::Array(_) => ValueType::Array,
                    ExpressionKind::InterpolatedString(_) => ValueType::String,
                    _ => ValueType::Mixed,
                }
            }
        }
    }
}
