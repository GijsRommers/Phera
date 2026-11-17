//! Call checking: `strlen()` has too few arguments; `$unknown()` is left alone.
//! Spread arguments make counts uncertain, and variadics accept extra named arguments.

use super::*;

struct ResolvedCall {
    signature: Option<crate::index::Signature>,
    label: String,
    intrinsic: Option<String>,
    kind: CallKind,
}

impl Analyzer<'_> {
    pub fn call(
        &mut self,
        callee: &Expression,
        arguments: &Arguments,
        scope: &mut Scope,
    ) -> ValueType {
        let resolved = if let ExpressionKind::Name(name) = &callee.kind
            && callee.span == name.span
        {
            self.named_call(name, scope)
        } else {
            let ty = self.expression(callee, scope, false);
            if ty == ValueType::Never {
                return ty;
            }
            ResolvedCall {
                signature: if let ValueType::Callable(signature) = ty {
                    Some(*signature)
                } else {
                    None
                },
                label: "callable".into(),
                intrinsic: None,
                kind: CallKind::Callable,
            }
        };
        let evaluation = self.arguments(
            arguments,
            resolved.signature.as_ref(),
            &resolved.label,
            scope,
            callee.span,
            resolved.kind,
        );
        if evaluation == Evaluation::Never {
            return ValueType::Never;
        }
        if !matches!(arguments, Arguments::FirstClassCallable) {
            if let Some(signature) = &resolved.signature {
                self.unbound_invocation(signature);
            }
            self.apply_global_effects(resolved.signature.as_ref(), scope);
        }
        if let Some(name) = resolved.intrinsic {
            self.intrinsic(&name, arguments, scope);
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            resolved
                .signature
                .map_or(ValueType::Mixed, |s| ValueType::Callable(Box::new(s)))
        } else {
            resolved.signature.map_or(ValueType::Mixed, |s| s.returns)
        }
    }

    fn named_call(&mut self, name: &Name, scope: &Scope) -> ResolvedCall {
        let candidates = self.context.names.resolve(name, UseKind::Function);
        let found = candidates.iter().find_map(|name| {
            self.index
                .functions
                .get(&name.to_ascii_lowercase())
                .map(|signature| (name, signature))
        });
        if found.is_none()
            && !candidates.iter().any(|name| {
                self.context
                    .known
                    .contains(&Symbol::Function(name.to_ascii_lowercase()))
                    || scope
                        .known
                        .contains(&Symbol::Function(name.to_ascii_lowercase()))
            })
        {
            self.report(Finding::UnknownFunction(candidates[0].clone()), name.span);
        }
        let intrinsic = found
            .filter(|(name, _)| {
                self.index
                    .builtin_functions
                    .contains(&name.to_ascii_lowercase())
            })
            .map(|(name, _)| name.to_string());
        ResolvedCall {
            signature: found.map(|(_, signature)| signature.clone()),
            label: format!("{}()", found.map_or(&candidates[0], |(name, _)| name)),
            intrinsic,
            kind: CallKind::Direct,
        }
    }

    pub fn method_call(
        &mut self,
        ty: &ValueType,
        name: &MemberName,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
        access: Access,
    ) -> ValueType {
        let (ValueType::Class(class), MemberName::Identifier(method)) = (ty, name) else {
            if let MemberName::Expression(e) = name
                && self.expression(e, scope, false) == ValueType::Never
            {
                return ValueType::Never;
            }
            return if self.arguments(arguments, None, "method", scope, span, CallKind::Direct)
                == Evaluation::Never
            {
                ValueType::Never
            } else {
                ValueType::Mixed
            };
        };
        let signature = self.index.method(class, &method.name).cloned();
        self.validate_method(class, &method.name, signature.as_ref(), span, access);
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
        let evaluation = self.arguments(
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
        if evaluation == Evaluation::Never {
            return ValueType::Never;
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            signature.map_or(ValueType::Mixed, |s| ValueType::Callable(Box::new(s)))
        } else {
            signature.map_or(ValueType::Mixed, |s| self.relative_type(s.returns, class))
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
        } else if self.index.complete(class)
            && access.reports_missing()
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
        {
            self.report(Finding::UnknownMethod(class.into(), method.into()), span);
        }
    }

    pub fn new_object(
        &mut self,
        target: &NewTarget,
        arguments: &Arguments,
        scope: &mut Scope,
    ) -> ValueType {
        let NewTarget::Class(expression) = target else {
            if let NewTarget::Anonymous(class) = target {
                let name = crate::collect::anonymous_name(&self.file.path, class.span.start);
                let signature = self.index.method(&name, "__construct").cloned();
                let evaluation = self.arguments(
                    arguments,
                    signature.as_ref(),
                    "anonymous class",
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
        let signature = class
            .as_ref()
            .and_then(|c| self.index.method(c, "__construct"))
            .cloned();
        let late_static = matches!(&expression.kind, ExpressionKind::Name(n)
            if n.text.eq_ignore_ascii_case("static"));
        if !late_static
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
        self.apply_global_effects(signature.as_ref(), scope);
        class.map_or(ValueType::Mixed, ValueType::Class)
    }
}

impl Analyzer<'_> {
    fn apply_global_effects(&self, signature: Option<&crate::index::Signature>, scope: &mut Scope) {
        if self.context.scope_kind == ScopeKind::Global
            && signature
                .is_none_or(|s| s.global_effects == crate::global_effects::GlobalEffects::Unknown)
        {
            scope.unknown_writes();
        }
    }
}
