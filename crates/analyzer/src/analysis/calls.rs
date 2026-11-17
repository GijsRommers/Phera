//! Call checking: `strlen()` has too few arguments; `$unknown()` is left alone.
//! Spread arguments make counts uncertain, and variadics accept extra named arguments.

use super::*;
use crate::index::Signature;

impl Analyzer<'_> {
    pub fn call(
        &mut self,
        callee: &Expression,
        arguments: &Arguments,
        scope: &mut Scope,
    ) -> ValueType {
        let (signature, label) = if let ExpressionKind::Name(name) = &callee.kind {
            let candidates = self.context.names.resolve(name, UseKind::Function);
            let found = candidates
                .iter()
                .find_map(|n| self.index.functions.get(&n.to_lowercase()));
            if found.is_none() {
                self.report(Finding::UnknownFunction(candidates[0].clone()), callee.span);
            }
            if let Some(builtin) = candidates.iter().find(|n| !n.contains('\\')) {
                self.intrinsic(builtin, arguments, scope);
            }
            (found.cloned(), format!("{}()", candidates[0]))
        } else {
            let ty = self.expression(callee, scope, false);
            (
                if let ValueType::Callable(signature) = ty {
                    Some(*signature)
                } else {
                    None
                },
                "callable".into(),
            )
        };
        self.arguments(arguments, signature.as_ref(), &label, scope, callee.span);
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
        static_call: bool,
    ) -> ValueType {
        let (ValueType::Class(class), MemberName::Identifier(method)) = (ty, name) else {
            if let MemberName::Expression(e) = name {
                self.expression(e, scope, false);
            }
            self.arguments(arguments, None, "method", scope, span);
            return ValueType::Mixed;
        };
        let signature = self.index.method(class, &method.name).cloned();
        if let Some(signature) = &signature {
            if static_call && !signature.modifiers.is_static {
                self.report(
                    Finding::StaticCall(class.clone(), method.name.clone()),
                    span,
                );
            }
        } else if self.index.class(class).is_some()
            && self
                .index
                .method(
                    class,
                    if static_call {
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
        self.arguments(
            arguments,
            signature.as_ref(),
            &format!("{class}::{}()", method.name),
            scope,
            span,
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
            self.arguments(arguments, None, "anonymous class", scope, Span::new(0, 0));
            return ValueType::Mixed;
        };
        let class = self.class_expression(expression, scope);
        let signature = class
            .as_ref()
            .and_then(|c| self.index.method(c, "__construct"))
            .cloned();
        if let Some(info) = class.as_ref().and_then(|c| self.index.class(c))
            && (info.modifiers.is_abstract || info.kind == ClassKind::Interface)
        {
            self.report(Finding::AbstractNew(info.name.clone()), expression.span);
        }
        self.arguments(
            arguments,
            signature.as_ref(),
            &format!("new {}", class.as_deref().unwrap_or("class")),
            scope,
            expression.span,
        );
        class.map_or(ValueType::Mixed, ValueType::Class)
    }

    pub fn arguments(
        &mut self,
        arguments: &Arguments,
        signature: Option<&Signature>,
        label: &str,
        scope: &mut Scope,
        span: Span,
    ) {
        let Arguments::List(arguments) = arguments else {
            return;
        };
        for (position, argument) in arguments.iter().enumerate() {
            let parameter = signature.and_then(|s| {
                if let Some(name) = &argument.name {
                    s.parameters.iter().find(|p| p.name == name.name)
                } else {
                    s.parameters.get(position)
                }
            });
            if parameter.is_some_and(|p| p.by_reference) {
                self.ensure_reference(&argument.value, scope);
            }
            self.expression(&argument.value, scope, false);
        }
        let Some(signature) = signature else {
            return;
        };
        let variadic = signature.parameters.iter().any(|p| p.variadic);
        let spread = arguments.iter().any(|a| a.spread);
        let mut supplied = std::collections::BTreeSet::new();
        for (position, argument) in arguments.iter().enumerate() {
            if let Some(name) = &argument.name {
                if let Some(i) = signature
                    .parameters
                    .iter()
                    .position(|p| p.name == name.name)
                {
                    supplied.insert(i);
                } else if !variadic {
                    self.report(
                        Finding::NamedArgument(label.into(), name.name.clone()),
                        span,
                    );
                }
            } else if !argument.spread {
                supplied.insert(position);
            }
        }
        if spread {
            return;
        }
        let missing = signature
            .parameters
            .iter()
            .enumerate()
            .any(|(i, p)| !p.optional && !p.variadic && !supplied.contains(&i));
        if missing
            || (!variadic
                && !signature.accepts_extra
                && arguments.len() > signature.parameters.len())
        {
            let required = signature
                .parameters
                .iter()
                .filter(|p| !p.optional && !p.variadic)
                .count();
            self.report(
                Finding::ArgumentCount(format!(
                    "{label} called with {} arguments; {} required, \
                {} accepted{}.",
                    arguments.len(),
                    required,
                    signature.parameters.len(),
                    if variadic { " or more" } else { "" }
                )),
                span,
            );
        }
    }

    pub fn property(
        &mut self,
        ty: &ValueType,
        name: &MemberName,
        scope: &mut Scope,
        span: Span,
        quiet: bool,
    ) -> ValueType {
        if let (ValueType::Class(class), MemberName::Identifier(name)) = (ty, name) {
            self.named_property(class, &name.name, span, quiet)
        } else {
            if let MemberName::Expression(e) = name {
                self.expression(e, scope, false);
            }
            ValueType::Mixed
        }
    }

    pub fn named_property(
        &mut self,
        class: &str,
        name: &str,
        span: Span,
        quiet: bool,
    ) -> ValueType {
        if let Some(property) = self.index.property(class, name) {
            self.relative_type(property.ty.clone(), class)
        } else {
            if !quiet && self.index.class(class).is_some() && !self.index.open_properties(class) {
                self.report(Finding::UnknownProperty(class.into(), name.into()), span);
            }
            ValueType::Mixed
        }
    }

    fn relative_type(&self, ty: ValueType, class: &str) -> ValueType {
        match &ty {
            ValueType::Class(name) if matches!(name.to_lowercase().as_str(), "self" | "static") => {
                ValueType::Class(class.into())
            }
            _ => ty,
        }
    }

    fn intrinsic(&mut self, name: &str, arguments: &Arguments, scope: &mut Scope) {
        let Arguments::List(arguments) = arguments else {
            return;
        };
        match name.to_lowercase().as_str() {
            "extract" | "parse_str" if arguments.len() == 1 => scope.dynamic = true,
            "extract" => scope.dynamic = true,
            "compact" => {
                for argument in arguments {
                    let mut values = vec![&argument.value];
                    while let Some(value) = values.pop() {
                        match &value.kind {
                            ExpressionKind::Literal(Literal::String(bytes)) => {
                                if let Ok(name) = std::str::from_utf8(bytes) {
                                    self.read(name, scope, argument.span, false);
                                }
                            }
                            ExpressionKind::Array(items) => {
                                values.extend(items.iter().map(|i| &i.value))
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
