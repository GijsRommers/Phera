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
                    if self.expression(item, scope, true) == ValueType::Never {
                        return ValueType::Never;
                    }
                }
                ValueType::Bool
            }
            ExpressionKind::Empty(item) => {
                if self.expression(item, scope, true) == ValueType::Never {
                    return ValueType::Never;
                }
                ValueType::Bool
            }
            ExpressionKind::Binary {
                operator: BinaryOperator::And | BinaryOperator::Or,
                ..
            } => {
                let Some(after) = self
                    .condition(expression, std::mem::take(scope), quiet)
                    .after()
                else {
                    return ValueType::Never;
                };
                *scope = after;
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
                if self.expression(value, scope, quiet) == ValueType::Never {
                    return ValueType::Never;
                }
                if self.class_expression(class, scope).is_never() {
                    return ValueType::Never;
                }
                ValueType::Bool
            }
            _ => ValueType::Mixed,
        }
    }
}
