//! Call expression routing: function, object, static and constructor syntax differ.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.
//! For example, `C::run()` and `$object->run()` use different static-call rules.

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
                let binding = self.pending_binding.take();
                self.prepare_inline_receiver(target, name, arguments, scope);
                let ty = self.expression(target, scope, quiet);
                self.pending_binding = binding;
                if ty == ValueType::Never {
                    return ty;
                }
                let access = match (&target.kind, name) {
                    (
                        ExpressionKind::Variable(Variable::Named(receiver)),
                        MemberName::Identifier(name),
                    ) if scope.known.contains(&Symbol::Method(
                        receiver.clone(),
                        name.name.to_ascii_lowercase(),
                    )) =>
                    {
                        Access::Inferred
                    }
                    _ => Access::object(target),
                };
                self.method_call(&ty, name, arguments, scope, expression.span, access)
            }
            ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } => {
                let class_name = self.class_expression(class, scope);
                if class_name.is_never() {
                    return ValueType::Never;
                }
                let class_name = class_name.name();
                let forwarding = self.forwards_object(class_name.as_deref());
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

    fn forwards_object(&self, class_name: Option<&str>) -> bool {
        self.context.object
            && self.context.this_class.as_ref().is_none_or(|object| {
                class_name.is_some_and(|class| {
                    self.index
                        .ancestry(object)
                        .iter()
                        .any(|ancestor| ancestor.name.eq_ignore_ascii_case(class))
                })
            })
    }
}
