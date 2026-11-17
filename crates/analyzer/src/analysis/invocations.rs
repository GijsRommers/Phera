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
                let forwarding = self.context.object
                    && self.context.this_class.as_ref().is_none_or(|object| {
                        class_name.as_ref().is_some_and(|class| {
                            self.index
                                .ancestry(object)
                                .iter()
                                .any(|ancestor| ancestor.name.eq_ignore_ascii_case(class))
                        })
                    });
                let ty = class_name.map_or(ValueType::Mixed, ValueType::Class);
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    if matches!(&class.kind, ExpressionKind::Name(name)
                        if name.text.eq_ignore_ascii_case("static"))
                    {
                        Access::LateStatic {
                            has_object: forwarding,
                        }
                    } else if forwarding {
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
