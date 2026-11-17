//! Call checking: `strlen()` has too few arguments; `$unknown()` is left alone.
//! Spread arguments make counts uncertain, and variadics accept extra named arguments.

use super::*;

impl Analyzer<'_> {
    pub fn call(
        &mut self,
        callee: &Expression,
        arguments: &Arguments,
        scope: &mut Scope,
    ) -> ValueType {
        let kind = if matches!(callee.kind, ExpressionKind::Name(_)) {
            CallKind::Direct
        } else {
            CallKind::Callable
        };
        let (signature, label, intrinsic) = if let ExpressionKind::Name(name) = &callee.kind {
            let candidates = self.context.names.resolve(name, UseKind::Function);
            let found = candidates.iter().find_map(|n| {
                self.index
                    .functions
                    .get(&n.to_lowercase())
                    .map(|signature| (n, signature))
            });
            if found.is_none()
                && !candidates
                    .iter()
                    .any(|n| scope.known.contains(&Symbol::Function(n.to_lowercase())))
            {
                self.report(Finding::UnknownFunction(candidates[0].clone()), callee.span);
            }
            let intrinsic = found
                .filter(|(name, _)| self.index.builtin_functions.contains(&name.to_lowercase()))
                .map(|(name, _)| name.to_string());
            let label = format!("{}()", found.map_or(&candidates[0], |(name, _)| name));
            (
                found.map(|(_, signature)| signature.clone()),
                label,
                intrinsic,
            )
        } else {
            let ty = self.expression(callee, scope, false);
            (
                if let ValueType::Callable(signature) = ty {
                    Some(*signature)
                } else {
                    None
                },
                "callable".into(),
                None,
            )
        };
        self.arguments(
            arguments,
            signature.as_ref(),
            &label,
            scope,
            callee.span,
            kind,
        );
        if let Some(name) = intrinsic {
            self.intrinsic(&name, arguments, scope);
        }
        if matches!(arguments, Arguments::FirstClassCallable) {
            signature.map_or(ValueType::Mixed, |s| ValueType::Callable(Box::new(s)))
        } else {
            signature.map_or(ValueType::Mixed, |s| s.returns)
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
            if let MemberName::Expression(e) = name {
                self.expression(e, scope, false);
            }
            self.arguments(arguments, None, "method", scope, span, CallKind::Direct);
            return ValueType::Mixed;
        };
        let signature = self.index.method(class, &method.name).cloned();
        if let Some(signature) = &signature {
            if matches!(access, Access::Static) && !signature.modifiers.is_static {
                self.report(
                    Finding::StaticCall(class.clone(), method.name.clone()),
                    span,
                );
            }
        } else if self.index.complete(class)
            && access.reports_missing()
            && self
                .index
                .class(class)
                .is_some_and(|c| c.kind != ClassKind::Trait)
            && self
                .index
                .method(
                    class,
                    if matches!(access, Access::Static) {
                        "__callStatic"
                    } else {
                        "__call"
                    },
                )
                .is_none()
        {
            self.report(
                Finding::UnknownMethod(class.clone(), method.name.clone()),
                span,
            );
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
        self.arguments(
            arguments,
            parameters,
            &format!("{class}::{}()", method.name),
            scope,
            span,
            CallKind::Direct,
        );
        if matches!(arguments, Arguments::FirstClassCallable) {
            signature.map_or(ValueType::Mixed, |s| ValueType::Callable(Box::new(s)))
        } else {
            signature.map_or(ValueType::Mixed, |s| self.relative_type(s.returns, class))
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
                self.arguments(
                    arguments,
                    signature.as_ref(),
                    "anonymous class",
                    scope,
                    class.span,
                    CallKind::Direct,
                );
                self.class_declaration(class);
                return ValueType::Class(name);
            }
            return ValueType::Mixed;
        };
        let class = self.class_expression(expression, scope);
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
        self.arguments(
            arguments,
            signature.as_ref(),
            &format!("new {}", class.as_deref().unwrap_or("class")),
            scope,
            expression.span,
            CallKind::Direct,
        );
        class.map_or(ValueType::Mixed, ValueType::Class)
    }
}
