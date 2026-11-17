//! Conditional expressions: `isset($x)` probes without reading an undefined variable.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;

impl Analyzer<'_> {
    pub fn logical(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Isset(items) => {
                for item in items {
                    self.expression(item, scope, true);
                }
                ValueType::Bool
            }
            ExpressionKind::Empty(item) => {
                self.expression(item, scope, true);
                ValueType::Bool
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => self.binary(*operator, left, right, scope, quiet),
            ExpressionKind::Ternary {
                condition,
                then,
                otherwise,
            } => self.ternary(condition, then.as_deref(), otherwise, scope),
            ExpressionKind::InstanceOf { value, class } => {
                self.expression(value, scope, quiet);
                self.class_expression(class, scope);
                ValueType::Bool
            }
            _ => ValueType::Mixed,
        }
    }
}
