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
            ExpressionKind::Call {
                callee, arguments, ..
            } => self.call(callee, arguments, scope),
            ExpressionKind::New { class, arguments } => self.new_object(class, arguments, scope),
            ExpressionKind::MethodCall {
                target,
                name,
                arguments,
                nullsafe,
            } => {
                let binding = self.pending_binding.take();
                self.prepare_inline_receiver(target, name, arguments, scope);
                let mut ty = self.expression(target, scope, quiet);
                self.pending_binding = binding;
                if ty == ValueType::Never {
                    return ty;
                }
                if self.level >= 2 && *nullsafe && ty == ValueType::Null {
                    if let MemberName::Identifier(method) = name {
                        self.method_receiver(
                            &ty,
                            &method.name,
                            expression.span,
                            Access::object(target),
                        );
                    }
                    return ty;
                }
                if self.level < 2
                    && matches!(&target.kind, ExpressionKind::Variable(Variable::Named(receiver)) if receiver == "this")
                    && let MemberName::Identifier(method) = name
                    && let ValueType::Class(narrowed) = scope.get("this").ty
                    && self.index.method(&narrowed, &method.name).is_some()
                {
                    // A guarded interface adds its members to $this; the
                    // original class still supplies its other members.
                    ty = ValueType::Class(narrowed);
                }
                let access = if !scope.known.is_empty()
                    && let MemberName::Identifier(name) = name
                    && let Some(receiver) = crate::scope::Place::of(target)
                    && scope
                        .known
                        .contains(&Symbol::Method(receiver, name.name.to_ascii_lowercase()))
                {
                    Access::Guarded
                } else {
                    Access::object(target)
                };
                let result = self.method_call(&ty, name, arguments, scope, expression.span, access);
                if self.level >= 2
                    && result == ValueType::Void
                    && !scope.facts.is_empty()
                    && let Some(receiver) = crate::scope::Place::of(target)
                {
                    scope.facts.forget_receiver(&receiver);
                }
                result
            }
            ExpressionKind::StaticCall {
                class,
                name,
                arguments,
            } => {
                if let Some(result) =
                    self.parent_hook_call(class, name, arguments, scope, expression.span)
                {
                    return result;
                }
                let class_name = self.class_expression(class, scope);
                if class_name.is_never() {
                    return ValueType::Never;
                }
                let class_name = class_name.name();
                let forwarding = self.forwards_object(class_name.as_deref());
                let ty = class_name.map_or(ValueType::Mixed, ValueType::Class);
                let guarded = match name {
                    MemberName::Identifier(name) if self.level >= 2 && !scope.known.is_empty() => {
                        crate::scope::Place::of(class).is_some_and(|receiver| {
                            scope
                                .known
                                .contains(&Symbol::Method(receiver, name.name.to_ascii_lowercase()))
                        })
                    }
                    _ => false,
                };
                self.method_call(
                    &ty,
                    name,
                    arguments,
                    scope,
                    expression.span,
                    if guarded {
                        Access::GuardedStatic
                    } else if matches!(&class.kind, ExpressionKind::Name(name)
                        if name.text.eq_ignore_ascii_case("static"))
                    {
                        Access::LateStatic {
                            has_object: forwarding,
                        }
                    } else {
                        Access::Static {
                            has_object: forwarding,
                            late_bound: matches!(&class.kind, ExpressionKind::Name(name)
                                if name.text.eq_ignore_ascii_case("self") || name.text.eq_ignore_ascii_case("parent")),
                        }
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
