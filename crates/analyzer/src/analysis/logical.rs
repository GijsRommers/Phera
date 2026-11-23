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
                    self.constant_null_check(item, scope, dead_code::NullCheck::Isset);
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
                let branches = self.condition(expression, std::mem::take(scope), quiet);
                let truth = branches.truth();
                let Some(after) = branches.after() else {
                    return ValueType::Never;
                };
                *scope = after;
                if self.level >= 4 {
                    truth.map_or(ValueType::Bool, ValueType::BoolLiteral)
                } else {
                    ValueType::Bool
                }
            }
            ExpressionKind::Binary {
                operator,
                left,
                right,
            } => self.binary(*operator, left, right, scope, quiet, expression),
            ExpressionKind::Ternary {
                condition,
                then,
                otherwise,
            } => self.ternary(condition, then.as_deref(), otherwise, scope),
            ExpressionKind::InstanceOf { value, class } => {
                let given = self.expression(value, scope, quiet);
                if given == ValueType::Never {
                    return ValueType::Never;
                }
                match self.class_expression(class, scope) {
                    ClassTarget::Never => return ValueType::Never,
                    ClassTarget::Known(resolved)
                        if self.level >= 4
                            && (!matches!(&class.kind, ExpressionKind::Name(name) if name.text.eq_ignore_ascii_case("static"))
                                || self
                                    .index
                                    .class(&resolved)
                                    .is_some_and(|class| class.modifiers.is_final)) =>
                    {
                        if let Some(ty) = self.constant_instanceof(
                            value,
                            &given,
                            resolved,
                            scope,
                            expression.span,
                        ) {
                            return ty;
                        }
                    }
                    _ => {}
                }
                ValueType::Bool
            }
            _ => ValueType::Mixed,
        }
    }
}
