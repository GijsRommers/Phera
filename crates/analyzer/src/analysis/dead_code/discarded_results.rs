//! A standalone `strlen($name);` discards a pure result.
//! Calling an unknown function or constructing a class can have effects.

use super::*;
use crate::index::Signature;

impl Analyzer<'_> {
    pub fn expression_statement(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
    ) -> ValueType {
        let previous = self.discarded_expression;
        if self.level >= 4 {
            self.discarded_expression = Some(expression.span);
        }
        let ty = self.expression(expression, scope, false);
        self.discarded_expression = previous;
        if self.level >= 4
            && ty != ValueType::Never
            && is_effect_free(expression)
            && let Some(source) = self
                .file
                .source
                .get(expression.span.start..expression.span.end)
        {
            self.diagnose_dead_code(
                "expr.resultUnused",
                expression.span,
                format!("Expression \"{source}\" on a separate line does not do anything."),
            );
        }
        ty
    }

    pub fn discarded_function_result(
        &mut self,
        span: Span,
        label: &str,
        intrinsic: Option<&str>,
        signature: Option<&Signature>,
    ) {
        if self.discarded_expression == Some(span)
            && (signature.is_some_and(|signature| signature.pure)
                || intrinsic.is_some_and(builtin_is_pure))
        {
            self.diagnose_dead_code(
                "function.resultUnused",
                span,
                format!("Call to function {label} on a separate line has no effect."),
            );
        }
    }

    pub fn discarded_method_result(
        &mut self,
        span: Span,
        class: &str,
        method: &str,
        signature: &Signature,
        access: Access,
    ) {
        if self.discarded_expression != Some(span) || signature.returns == ValueType::Never {
            return;
        }
        let reflection_query = class.eq_ignore_ascii_case("ReflectionClass")
            && (method.eq_ignore_ascii_case("getConstants")
                || method.eq_ignore_ascii_case("getDefaultProperties"));
        if signature.pure || reflection_query {
            let (identifier, description) = if access.static_call() {
                ("staticMethod.resultUnused", "static method")
            } else {
                ("method.resultUnused", "method")
            };
            self.diagnose_dead_code(
                identifier,
                span,
                format!(
                    "Call to {description} {class}::{method}() on a separate line has no effect."
                ),
            );
        }
    }

    pub fn discarded_constructor_result(&mut self, span: Span, class: &str) {
        if self.discarded_expression == Some(span)
            && self.index.class(class).is_some_and(|class| {
                class.kind == ClassKind::Class
                    && !class.modifiers.is_abstract
                    && !self
                        .index
                        .builtin_classes
                        .contains(&class.name.to_ascii_lowercase())
            })
            && self.index.complete(class)
            && self.index.method(class, "__destruct").is_none()
            && self
                .index
                .method(class, "__construct")
                .is_none_or(|signature| signature.pure)
        {
            self.diagnose_dead_code(
                "new.resultUnused",
                span,
                format!("Call to new {class}() on a separate line has no effect."),
            );
        }
    }
}

fn builtin_is_pure(name: &str) -> bool {
    super::super::refine_types::predicate_type(name).is_some()
        || matches!(
            name,
            "abs"
                | "array_keys"
                | "array_values"
                | "bin2hex"
                | "ceil"
                | "floor"
                | "ltrim"
                | "ord"
                | "rtrim"
                | "strlen"
                | "strtolower"
                | "strtoupper"
                | "str_contains"
                | "str_ends_with"
                | "str_starts_with"
                | "substr"
                | "trim"
        )
}

fn is_effect_free(expression: &Expression) -> bool {
    let mut pending = vec![expression];
    while let Some(expression) = pending.pop() {
        match &expression.kind {
            ExpressionKind::Literal(_)
            | ExpressionKind::MagicConstant(_)
            | ExpressionKind::Variable(Variable::Named(_)) => {}
            ExpressionKind::Array(_) => pending.extend(crate::walk::children(expression)),
            ExpressionKind::Unary {
                operator: UnaryOperator::Not,
                operand,
            } => pending.push(operand),
            _ => return false,
        }
    }
    true
}
