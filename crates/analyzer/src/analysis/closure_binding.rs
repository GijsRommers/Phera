//! Explicit closure rebinding: `Closure::bind(function () { $this->run(); }, $service)`.
//! The body uses the bound object type rather than assuming its lexical owner.

use super::*;

#[derive(Clone)]
pub struct ClosureBinding {
    object: bool,
    this_class: Option<String>,
    scope_class: Option<String>,
}

impl Analyzer<'_> {
    pub fn prepare_binding(
        &mut self,
        class: &str,
        method: &str,
        arguments: &Arguments,
        scope: &Scope,
    ) {
        if !class.eq_ignore_ascii_case("Closure") || !method.eq_ignore_ascii_case("bind") {
            return;
        }
        let Arguments::List(arguments) = arguments else {
            return;
        };
        let (Some(closure), Some(object)) = (
            argument(arguments, 0, "closure"),
            argument(arguments, 1, "newThis"),
        ) else {
            return;
        };
        let mut this_class = self.binding_class(object, scope);
        let scope_class = argument(arguments, 2, "newScope")
            .map_or(self.context.class.clone(), |value| {
                self.binding_class(value, scope)
            });
        // A runtime-selected scope on an interface-typed receiver can
        // expose methods from its concrete class that the interface omits.
        if scope_class.is_none()
            && this_class
                .as_ref()
                .and_then(|class| self.index.class(class))
                .is_some_and(|class| class.kind == ClassKind::Interface)
        {
            this_class = None;
        }
        let binding = ClosureBinding {
            object: !matches!(object.kind, ExpressionKind::Literal(Literal::Null)),
            this_class,
            scope_class,
        };
        self.pending_binding = Some((closure.span.start, binding));
    }

    pub fn prepare_inline_receiver(
        &mut self,
        closure: &Expression,
        method: &MemberName,
        arguments: &Arguments,
        scope: &Scope,
    ) {
        if !is_closure(closure) {
            return;
        }
        let (MemberName::Identifier(method), Arguments::List(arguments)) = (method, arguments)
        else {
            return;
        };
        let bind_to = method.name.eq_ignore_ascii_case("bindTo");
        if !(bind_to || method.name.eq_ignore_ascii_case("call")) {
            return;
        }
        if let Some(object) = argument(arguments, 0, "newThis") {
            let this_class = self.binding_class(object, scope);
            let scope_class = bind_to
                .then(|| argument(arguments, 1, "newScope"))
                .flatten()
                .and_then(|value| self.binding_class(value, scope))
                .or_else(|| this_class.clone());
            self.pending_binding = Some((
                closure.span.start,
                ClosureBinding {
                    object: !matches!(object.kind, ExpressionKind::Literal(Literal::Null)),
                    this_class,
                    scope_class,
                },
            ));
        }
    }

    pub fn prepare_documented_binding(&mut self, closure: &Expression, class: &str) {
        if is_closure(closure) {
            self.pending_binding = Some((
                closure.span.start,
                ClosureBinding {
                    object: true,
                    this_class: Some(class.into()),
                    scope_class: Some(class.into()),
                },
            ));
        }
    }

    pub fn prepare_local_rebinding(
        &mut self,
        statement: &Statement,
        next: &Statement,
        scope: &Scope,
    ) {
        // A closure stored and immediately rebound can refer to the new
        // object's members. A later or conditional bind cannot establish this.
        if let Some((name, closure)) = assigned_value(statement)
            && is_closure(closure)
            && let Some((_, call)) = assigned_value(next)
            && let ExpressionKind::MethodCall {
                target,
                name: method,
                arguments,
                nullsafe: false,
            } = &call.kind
            && matches!(&target.kind, ExpressionKind::Variable(Variable::Named(target)) if target == name)
            && matches!(method, MemberName::Identifier(method) if method.name.eq_ignore_ascii_case("bindTo"))
        {
            self.prepare_inline_receiver(closure, method, arguments, scope);
        }
    }

    pub fn apply_binding(&mut self, span: Span, static_closure: bool) {
        if let Some((offset, binding)) = &self.pending_binding
            && *offset == span.start
        {
            self.context.object = binding.object && !static_closure;
            self.context.this_class = if self.context.object {
                binding.this_class.clone()
            } else {
                None
            };
            self.context.class = binding.scope_class.clone();
        }
    }

    fn binding_class(&mut self, expression: &Expression, scope: &Scope) -> Option<String> {
        match &expression.kind {
            ExpressionKind::Variable(Variable::Named(name)) if name == "this" => {
                self.context.this_class.clone()
            }
            ExpressionKind::Variable(Variable::Named(name)) => match scope.get(name).ty {
                ValueType::Class(name) => Some(name),
                _ => None,
            },
            ExpressionKind::Literal(Literal::String(bytes)) => {
                std::str::from_utf8(bytes).ok().and_then(|s| {
                    if s.eq_ignore_ascii_case("static") {
                        self.context.class.clone()
                    } else {
                        Some(s.trim_start_matches('\\').into())
                    }
                })
            }
            ExpressionKind::New {
                class: NewTarget::Class(class),
                ..
            }
            | ExpressionKind::ClassConstant { class, .. } => {
                if let ExpressionKind::Name(name) = &class.kind {
                    self.class_name(name, false)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

pub fn is_closure(expression: &Expression) -> bool {
    matches!(
        expression.kind,
        ExpressionKind::Closure(_) | ExpressionKind::ArrowFunction(_)
    )
}

fn argument<'a>(arguments: &'a [Argument], position: usize, name: &str) -> Option<&'a Expression> {
    arguments.iter().enumerate().find_map(|(index, argument)| {
        argument
            .name
            .as_ref()
            .map_or(index == position, |parameter| parameter.name == name)
            .then_some(&argument.value)
    })
}

fn assigned_value(statement: &Statement) -> Option<(&str, &Expression)> {
    if let StatementKind::Expression(expression) = &statement.kind
        && let ExpressionKind::Assign { target, value } = &expression.kind
        && let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
    {
        return Some((name, value));
    }
    None
}
