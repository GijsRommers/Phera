//! Member expression facts: `self::MISSING` checks the indexed class constants.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;
use phera_source::deprecations::ItemKind;

impl Analyzer<'_> {
    pub fn member_expressions(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ValueType {
        match &expression.kind {
            ExpressionKind::Property { target, name, .. } => {
                let ty = self.expression(target, scope, quiet);
                if ty == ValueType::Never {
                    return ty;
                }
                self.property(
                    &ty,
                    name,
                    scope,
                    expression.span,
                    quiet,
                    Access::object(target),
                )
            }
            ExpressionKind::StaticProperty { class, name } => {
                self.static_property(class, name, scope, expression.span, quiet)
            }
            ExpressionKind::ClassConstant { class, name } => {
                self.class_constant(class, name, scope)
            }
            _ => ValueType::Mixed,
        }
    }
    fn static_property(
        &mut self,
        class: &Expression,
        name: &Variable,
        scope: &mut Scope,
        span: Span,
        quiet: bool,
    ) -> ValueType {
        let late_static = matches!(&class.kind, ExpressionKind::Name(name)
                    if name.text.eq_ignore_ascii_case("static"));
        let class = self.class_expression(class, scope);
        if class.is_never() {
            return ValueType::Never;
        }
        let class = class.name();
        if let Variable::Named(name) = name
            && let Some(class) = class
        {
            return self.named_property(
                &class,
                name,
                span,
                quiet
                    || (late_static
                        && self
                            .index
                            .class(&class)
                            .is_some_and(|c| c.modifiers.is_abstract)),
                PropertyAccess::Static,
            );
        }
        if let Variable::Dynamic(name) = name
            && self.expression(name, scope, false) == ValueType::Never
        {
            return ValueType::Never;
        }
        ValueType::Mixed
    }

    fn class_constant(
        &mut self,
        class: &Expression,
        name: &MemberName,
        scope: &mut Scope,
    ) -> ValueType {
        let late_static = matches!(&class.kind, ExpressionKind::Name(name)
                    if name.text.eq_ignore_ascii_case("static"));
        let class = self.class_expression(class, scope);
        if class.is_never() {
            return ValueType::Never;
        }
        let class = class.name();
        if let MemberName::Expression(name) = name
            && self.expression(name, scope, false) == ValueType::Never
        {
            return ValueType::Never;
        }
        if let (Some(class), MemberName::Identifier(name)) = (class, name)
            && !name.name.eq_ignore_ascii_case("class")
            && !scope
                .known
                .contains(&Symbol::Constant(Symbol::constant_name(&format!(
                    "{class}::{}",
                    name.name
                ))))
            && !self.index.constant(&class, &name.name)
        {
            if let Some(item) =
                self.omitted_builtin_member(ItemKind::ClassConstant, &class, &name.name)
            {
                self.report(Finding::NotSupported(item), name.span);
            } else if self.index.class(&class).is_some()
                && self.index.complete(&class)
                && !(late_static
                    && self
                        .index
                        .class(&class)
                        .is_some_and(|c| c.modifiers.is_abstract))
            {
                self.report(
                    Finding::UnknownClassConstant(format!("{class}::{}", name.name)),
                    name.span,
                );
            }
        }
        ValueType::Mixed
    }
}
