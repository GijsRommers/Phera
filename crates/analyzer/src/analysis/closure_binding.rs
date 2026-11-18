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
        let get = |position: usize, name: &str| {
            arguments
                .iter()
                .enumerate()
                .find(|(i, argument)| {
                    argument
                        .name
                        .as_ref()
                        .map_or(*i == position, |n| n.name == name)
                })
                .map(|(_, a)| &a.value)
        };
        let (Some(closure), Some(object)) = (get(0, "closure"), get(1, "newThis")) else {
            return;
        };
        let this_class = self.binding_class(object, scope);
        let scope_class = get(2, "newScope").map_or(self.context.class.clone(), |value| {
            self.binding_class(value, scope)
        });
        let binding = ClosureBinding {
            object: !matches!(object.kind, ExpressionKind::Literal(Literal::Null)),
            this_class,
            scope_class,
        };
        self.pending_binding = Some((closure.span.start, binding));
    }

    pub fn apply_binding(&mut self, span: Span) {
        if let Some((offset, binding)) = &self.pending_binding
            && *offset == span.start
        {
            self.context.object = binding.object;
            self.context.this_class = binding.this_class.clone();
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
