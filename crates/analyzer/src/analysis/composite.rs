//! Remaining expression effects: closures capture scopes and throw terminates a path.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.
//! For example, `[throw new Exception, $missing]` never reads the second item.

use super::chain_value::ChainValue;
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
            ExpressionKind::Index { .. } => {
                self.offset_chain(expression, scope, quiet).finish(scope)
            }
            ExpressionKind::Unary { operator, operand } if self.level >= 3 => {
                let ty = self.expression(operand, scope, quiet);
                if ty == ValueType::Never {
                    ty
                } else {
                    if self.level >= 4 && *operator == UnaryOperator::Not {
                        let truth = dead_code::truth(&ty);
                        self.constant_negation(expression, operand, truth);
                        if let Some(truth) = truth {
                            return ValueType::BoolLiteral(!truth);
                        }
                    }
                    super::operators::unary_type(*operator, ty)
                }
            }
            ExpressionKind::Cast { kind, operand } if self.level >= 2 => {
                let ty = self.expression(operand, scope, quiet);
                if ty == ValueType::Never {
                    return ty;
                }
                if self.strict_mixed(&ty) {
                    let target = match kind {
                        CastKind::Int => Some(("cast.int", "int")),
                        CastKind::Float => Some(("cast.double", "float")),
                        CastKind::String => Some(("cast.string", "string")),
                        _ => None,
                    };
                    if let Some((identifier, target)) = target {
                        self.diagnose(
                            identifier,
                            expression.span,
                            format!("Cannot cast mixed to {target}."),
                        );
                    }
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
            ExpressionKind::Match(value) => self.match_expression(value, scope, expression.span),
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
            ExpressionKind::Print(value) => {
                let ty = self.expression(value, scope, false);
                if ty == ValueType::Never {
                    return ty;
                }
                if self.strict_mixed(&ty) {
                    self.diagnose(
                        "print.nonString",
                        expression.span,
                        "Parameter mixed of print cannot be converted to string.".into(),
                    );
                }
                ValueType::Int
            }
            ExpressionKind::Throw(value) => {
                let ty = self.expression(value, scope, false);
                if ty == ValueType::Never {
                    return ty;
                }
                if self.strict_mixed(&ty) {
                    self.diagnose(
                        "throw.notThrowable",
                        expression.span,
                        "Invalid type mixed to throw.".into(),
                    );
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
    pub(super) fn offset_chain(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ChainValue {
        let ExpressionKind::Index { target, index } = &expression.kind else {
            return ChainValue::plain(ValueType::Mixed);
        };
        let mut chain = self.chain_expression(target, scope, quiet);
        if chain.ty == ValueType::Never {
            return chain;
        }
        let key = index.as_ref().map_or(ValueType::Mixed, |index| {
            self.expression(index, scope, quiet)
        });
        if key == ValueType::Never {
            chain.ty = key;
            return chain;
        }
        if index.is_some() {
            self.invalid_array_offset(&chain.ty, &key, expression.span);
            self.missing_array_offset(&chain.ty, &key, expression, scope, quiet);
        }
        if self.strict_mixed(&chain.ty) && !scope.facts.has_expression(expression) {
            self.mixed_offset(&chain.ty, index.as_ref().map(|_| &key), expression.span);
        }
        chain.ty = self.offset_value(&chain.ty, &key);
        chain
    }

    pub(super) fn offset_value(&self, target: &ValueType, key: &ValueType) -> ValueType {
        if self.strict_mixed(target) {
            target.clone()
        } else {
            super::arrays::offset_type(target, key)
        }
    }
}
