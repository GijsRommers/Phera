//! `public string $name { set { $this->name = $value; } }` gives the
//! hook a method scope and the setter its own parameter.

use super::*;
use phera_ast::{HookBody, PropertyHook};

impl Analyzer<'_> {
    pub(super) fn property_hooks(
        &mut self,
        property: &str,
        ty: Option<&Type>,
        hooks: &[PropertyHook],
    ) {
        for hook in hooks {
            let previous = self.context.clone();
            let exceptions = std::mem::take(&mut self.exception_scopes);
            let controls = std::mem::take(&mut self.controls);
            let returns = std::mem::take(&mut self.return_scopes);
            let setter = hook.name.name.eq_ignore_ascii_case("set");
            let property_type = ValueType::declared(ty, &self.context.names);
            self.context.scope_kind = ScopeKind::Function;
            self.context.incomplete = false;
            self.context.object = true;
            self.context.unbound_this = None;
            self.context.this_class.clone_from(&self.context.class);
            self.context.function = format!("${property}::{}", hook.name.name);
            self.context.declared_return = setter || ty.is_some();
            self.context.documented_return = None;
            self.context.inferred_return = None;
            self.context.generator = hook.is_generator;
            self.context.by_reference = hook.by_reference;
            self.context.returns = if setter {
                ValueType::Void
            } else {
                property_type.clone()
            };
            let mut scope = Scope::default();
            self.check_attributes(&hook.attributes, &mut scope);
            self.parameter_scope(&hook.parameters, &mut scope);
            if setter && !hook.has_parameter_list {
                let value_type = self.context.class.as_ref().map_or_else(
                    || property_type.clone(),
                    |class| self.relative_type(property_type.clone(), class),
                );
                scope.define("value", value_type);
            }
            match &hook.body {
                HookBody::None => {}
                HookBody::Expression(body) => {
                    let given = self.expression(body, &mut scope, false);
                    if !setter {
                        self.check_return_value(Some(body), &given, &scope, body.span);
                    }
                }
                HookBody::Block(body) => {
                    let flow = self.block(body, &mut scope);
                    self.check_return_path(flow, body, hook.span);
                }
            }
            self.context = previous;
            self.exception_scopes = exceptions;
            self.controls = controls;
            self.return_scopes = returns;
        }
    }

    /// `parent::$name::get()` invokes an instance hook, or reads the
    /// parent's backing slot if it has no hook.
    pub(super) fn parent_hook_call(
        &mut self,
        target: &Expression,
        method: &MemberName,
        arguments: &Arguments,
        scope: &mut Scope,
        span: Span,
    ) -> Option<ValueType> {
        let ExpressionKind::StaticProperty { class, name } = &target.kind else {
            return None;
        };
        let MemberName::Identifier(method) = method else {
            return None;
        };
        let getter = method.name.eq_ignore_ascii_case("get");
        if !(getter || method.name.eq_ignore_ascii_case("set"))
            || !matches!(&class.kind, ExpressionKind::Name(name) if name.text.eq_ignore_ascii_case("parent"))
            || self.file.source.as_bytes().get(target.span.start) == Some(&b'(')
        {
            return None;
        }
        let name = match name {
            Variable::Named(name) => std::borrow::Cow::Borrowed(name.as_str()),
            Variable::Dynamic(name) => match &name.kind {
                ExpressionKind::Literal(Literal::String(name)) => String::from_utf8_lossy(name),
                ExpressionKind::Literal(Literal::Int(value)) => value.to_string().into(),
                _ => return None,
            },
        };
        let parent = self.class_expression(class, scope).name();
        if self.arguments(arguments, None, &method.name, scope, span, CallKind::Direct)
            == Evaluation::Never
        {
            return Some(ValueType::Never);
        }
        if !getter {
            return Some(ValueType::Void);
        }
        Some(parent.map_or(ValueType::Mixed, |class| {
            self.named_property(&class, &name, span, true, PropertyAccess::Instance)
        }))
    }
}
