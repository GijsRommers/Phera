//! Call expression routing: function, object, static and constructor syntax differ.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;

impl Analyzer<'_> {
    pub fn invocations(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Call { callee, arguments } => self.call(callee, arguments, scope),
            ExpressionKind::New { class, arguments } => self.new_object(class, arguments, scope),
            ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                ..
            } => {
                let ty = self.expression(target, scope, quiet);
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    Access::object(target),
                )
            }
            ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } => {
                let class_name = self.class_expression(class, scope);
                let ty = class_name.map_or(ValueType::Mixed, ValueType::Class);
                let forwarding = matches!(&class.kind, ExpressionKind::Name(n)
                    if matches!(n.text.to_lowercase().as_str(), "self" | "static" | "parent"))
                    && self.context.object;
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    if forwarding {
                        Access::Forwarded
                    } else {
                        Access::Static
                    },
                )
            }
            _ => ValueType::Mixed,
        }
    }
}
