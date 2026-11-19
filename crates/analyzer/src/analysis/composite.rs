//! Remaining expression effects: closures capture scopes and throw terminates a path.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.
//! For example, `[throw new Exception, $missing]` never reads the second item.

use super::*;

impl Analyzer<'_> {
    pub fn composite(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Array(items) => self.array_literal(items, scope),
            ExpressionKind::Index { target, index } => {
                let target = self.expression(target, scope, quiet);
                if target == ValueType::Never {
                    return target;
                }
                let key = index.as_ref().map_or(ValueType::Mixed, |index| {
                    self.expression(index, scope, quiet)
                });
                if key == ValueType::Never {
                    return key;
                }
                super::arrays::offset_type(&target, &key)
            }
            ExpressionKind::Cast { kind, operand } if self.level >= 2 => {
                let ty = self.expression(operand, scope, quiet);
                if ty == ValueType::Never {
                    return ty;
                }
                match kind {
                    CastKind::Int => ValueType::Int,
                    CastKind::Float => ValueType::Float,
                    CastKind::String => ValueType::String,
                    CastKind::Array
                        if matches!(ty, ValueType::Shape(_) | ValueType::Collection(_)) =>
                    {
                        ty
                    }
                    CastKind::Array => ValueType::Array,
                    CastKind::Object => ValueType::Object,
                    CastKind::Bool => ValueType::Bool,
                    CastKind::Unset => ValueType::Null,
                    CastKind::Void => ValueType::Void,
                }
            }
            ExpressionKind::Match(expression) => self.match_expression(expression, scope),
            ExpressionKind::Clone { object, with } if self.level >= 2 => {
                let object = self.expression(object, scope, quiet);
                if object == ValueType::Never
                    || with
                        .as_ref()
                        .is_some_and(|with| self.expression(with, scope, quiet) == ValueType::Never)
                {
                    return ValueType::Never;
                }
                object
            }
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
                    ExpressionKind::InterpolatedString(_) => ValueType::String,
                    _ => ValueType::Mixed,
                }
            }
        }
    }
}
