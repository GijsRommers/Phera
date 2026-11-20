//! Call checking: `strlen()` has too few arguments; `$unknown()` is left alone.
//! Spread arguments make counts uncertain, and variadics accept extra named arguments.

use super::*;
use phera_source::deprecations::ItemKind;

impl Analyzer<'_> {
    pub fn method_call(
        &mut self,
        ty: &ValueType,
        name: &MemberName,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
        access: Access,
    ) -> ValueType {
        if let (ValueType::Callable(signature), MemberName::Identifier(method)) = (ty, name)
            && method.name.eq_ignore_ascii_case("__invoke")
        {
            return self.invoke_closure(signature, arguments, scope, span);
        }
        let receiver = match name {
            MemberName::Identifier(method) => self.method_receiver(ty, &method.name, span, access),
            _ => None,
        };
        let (Some(class), MemberName::Identifier(method)) = (receiver.as_deref(), name) else {
            if let MemberName::Expression(e) = name
                && self.expression(e, scope, false) == ValueType::Never
            {
                return ValueType::Never;
            }
            let invalid_mixed_receiver = matches!(name, MemberName::Identifier(_))
                && !matches!(access, Access::Guarded | Access::GuardedStatic)
                && self.strict_mixed(ty);
            let kind = if invalid_mixed_receiver {
                CallKind::InvalidReceiver
            } else {
                CallKind::Direct
            };
            return if self.arguments(arguments, None, "method", scope, span, kind)
                == Evaluation::Never
            {
                ValueType::Never
            } else if invalid_mixed_receiver {
                ValueType::ImplicitMixed
            } else {
                ValueType::Mixed
            };
        };
        self.dependencies.resolve(self.index, class);
        let late_class = access
            .late_bound()
            .then_some(self.context.class.as_deref())
            .flatten();
        let fallback_receiver = ValueType::Class(late_class.unwrap_or(class).into());
        let receiver = if late_class.is_none()
            && (ty.class_name().is_some() || matches!(ty, ValueType::Intersection(_)))
        {
            ty
        } else {
            &fallback_receiver
        };
        let signature = self
            .index
            .method_owner(class, &method.name)
            .map(|(owner, signature)| {
                let signature = signature.clone().typed(self.index);
                let signature = if self.level >= 2 {
                    signature.for_receiver(self.index, owner, receiver)
                } else {
                    signature
                };
                if self.level >= 10 {
                    signature.with_inherited_array_return(self.index, owner, &method.name, receiver)
                } else {
                    signature
                }
            })
            .or_else(|| self.index.doc_method(class, &method.name).cloned())
            .map(|mut signature| {
                for parameter in &mut signature.parameters {
                    if let Some(receiver) = parameter.closure_receiver.take() {
                        parameter.closure_receiver =
                            match self.relative_type(ValueType::Class(receiver), class) {
                                ValueType::Class(class) => Some(class),
                                _ => None,
                            };
                    }
                }
                signature
            });
        if signature.is_none()
            && let Some(item) = self.omitted_builtin_member(ItemKind::Method, class, &method.name)
            && self
                .index
                .method(
                    class,
                    if access.static_call() {
                        "__callStatic"
                    } else {
                        "__call"
                    },
                )
                .is_none()
        {
            self.report(Finding::NotSupported(item), span);
        } else {
            self.validate_method(class, &method.name, signature.as_ref(), span, access);
            self.check_internal_method(class, &method.name, span);
        }
        // The engine synthesizes Closure::__invoke from each actual closure. The stub
        // placeholder has no usable parameter list; inferred closures use their own signature.
        let parameters = if class.eq_ignore_ascii_case("Closure")
            && method.name.eq_ignore_ascii_case("__invoke")
        {
            None
        } else {
            signature.as_ref()
        };
        let binding = self.pending_binding.take();
        self.prepare_binding(class, &method.name, arguments, scope);
        let facts = self.argument_facts(
            arguments,
            parameters,
            &format!("{class}::{}()", method.name),
            scope,
            span,
            CallKind::Direct,
        );
        if !matches!(arguments, Arguments::FirstClassCallable) {
            self.apply_global_effects(signature.as_ref(), scope);
        }
        self.pending_binding = binding;
        if facts.evaluation == Evaluation::Never {
            return ValueType::Never;
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            signature.map_or(ValueType::Mixed, crate::index::Signature::as_closure)
        } else {
            if let Some(signature) = &signature {
                self.discarded_method_result(span, class, &method.name, signature, access);
            }
            let returns = if self.level >= 7
                && !access.static_call()
                && let Arguments::List(args) = arguments
                && args
                    .first()
                    .is_some_and(|arg| arg.name.is_none() && !arg.spread)
                && let Some(returns) = crate::framework_knowledge::doctrine::return_type(
                    self.index,
                    class,
                    &method.name,
                    &facts.types,
                ) {
                returns
            } else if let Some(signature) = &signature {
                let ty = signature.infer_return(arguments, &facts.types, self.index);
                self.declared_array_result(signature, ty)
            } else if self.level >= 2 && !matches!(access, Access::Guarded | Access::GuardedStatic)
            {
                let dispatcher = if access.static_call() {
                    "__callStatic"
                } else {
                    "__call"
                };
                self.index.method_owner(class, dispatcher).map_or(
                    ValueType::Mixed,
                    |(owner, signature)| {
                        let returns = signature
                            .clone()
                            .typed(self.index)
                            .for_receiver(self.index, owner, receiver)
                            .returns;
                        if returns == ValueType::Void {
                            ValueType::Mixed
                        } else {
                            returns
                        }
                    },
                )
            } else {
                ValueType::Mixed
            };
            let returns = if self.level >= 9 {
                crate::framework_knowledge::doctrine::query_result(
                    self.index,
                    class,
                    &method.name,
                    returns,
                    self.options.doctrine_stubs,
                )
            } else {
                returns
            };
            let returns = if self.options.symfony_stubs {
                crate::framework_knowledge::symfony::return_type(
                    self.index,
                    class,
                    &method.name,
                    arguments,
                    &facts.types,
                    returns,
                )
            } else {
                returns
            };
            self.relative_type(returns, class)
        }
    }

    fn validate_method(
        &mut self,
        class: &str,
        method: &str,
        signature: Option<&crate::index::Signature>,
        span: Span,
        access: Access,
    ) {
        if let Some(signature) = signature {
            if access.static_call() && !signature.modifiers.is_static {
                self.report(Finding::StaticCall(class.into(), method.into()), span);
            }
        } else if self.can_report_missing_method(class, method, access) {
            self.report(Finding::UnknownMethod(class.into(), method.into()), span);
        }
    }

    pub fn can_report_missing_method(&self, class: &str, method: &str, access: Access) -> bool {
        access.reports_missing(self.level)
            && self.index.complete(class)
            && !self
                .index
                .documented_method(class, method, access.static_call())
            && self.index.class(class).is_some_and(|c| {
                c.kind != ClassKind::Trait
                    && !(c.modifiers.is_abstract && access.dynamic_receiver())
            })
            && self
                .index
                .method(
                    class,
                    if access.static_call() {
                        "__callStatic"
                    } else {
                        "__call"
                    },
                )
                .is_none()
    }

    pub fn new_object(
        &mut self,
        target: &NewTarget,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
    ) -> ValueType {
        let NewTarget::Class(expression) = target else {
            if let NewTarget::Anonymous(class) = target {
                let name = crate::collect::anonymous_name(&self.file.path, class.span.start);
                let signature = self
                    .index
                    .method(&name, "__construct")
                    .map(|signature| signature.clone().typed(self.index));
                let evaluation = self.arguments(
                    arguments,
                    signature.as_ref(),
                    "new anonymous class",
                    scope,
                    class.span,
                    CallKind::Direct,
                );
                if evaluation == Evaluation::Never {
                    return ValueType::Never;
                }
                self.class_declaration(class);
                return ValueType::Class(name);
            }
            return ValueType::Mixed;
        };
        let class = self.class_expression(expression, scope);
        if class.is_never() {
            return ValueType::Never;
        }
        let class = class.name();
        let signature = class.as_ref().and_then(|class| {
            let (owner, signature) = self.index.method_owner(class, "__construct")?;
            Some(signature.clone().typed(self.index).for_receiver(
                self.index,
                owner,
                &ValueType::Class(class.clone()),
            ))
        });
        let late_static = matches!(&expression.kind, ExpressionKind::Name(n)
            if n.text.eq_ignore_ascii_case("static"));
        if late_static && let Some(class) = class.as_deref() {
            self.check_late_static_new(class, span);
        }
        if !late_static
            && matches!(expression.kind, ExpressionKind::Name(_))
            && let Some(info) = class.as_ref().and_then(|c| self.index.class(c))
            && (info.modifiers.is_abstract || info.kind == ClassKind::Interface)
        {
            let finding = if info.kind == ClassKind::Interface {
                Finding::InterfaceNew(info.name.clone())
            } else {
                Finding::AbstractNew(info.name.clone())
            };
            self.report(finding, expression.span);
        }
        let evaluation = self.arguments(
            arguments,
            signature.as_ref(),
            &format!("new {}", class.as_deref().unwrap_or("class")),
            scope,
            expression.span,
            CallKind::Direct,
        );
        if evaluation == Evaluation::Never {
            return ValueType::Never;
        }
        if let Some(class) = &class
            && !late_static
        {
            self.discarded_constructor_result(span, class);
        }
        self.apply_global_effects(signature.as_ref(), scope);
        class.map_or(ValueType::Mixed, ValueType::Class)
    }
}

impl Analyzer<'_> {
    pub fn apply_global_effects(
        &self,
        signature: Option<&crate::index::Signature>,
        scope: &mut Scope,
    ) {
        if self.context.scope_kind == ScopeKind::Global
            && signature
                .is_none_or(|s| s.global_effects == crate::global_effects::GlobalEffects::Unknown)
        {
            scope.unknown_writes();
        }
    }
}
