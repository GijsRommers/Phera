//! Member expression facts: `self::MISSING` checks the indexed class constants.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::chain_value::ChainValue;
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
            ExpressionKind::Property { .. } => {
                self.property_chain(expression, scope, quiet).finish(scope)
            }
            ExpressionKind::StaticProperty { class, name, .. } => {
                self.static_property(class, name, scope, expression.span, quiet)
            }
            ExpressionKind::ClassConstant { class, name } => {
                self.class_constant(class, name, scope)
            }
            _ => ValueType::Mixed,
        }
    }

    pub(super) fn property_chain(
        &mut self,
        expression: &Expression,
        scope: &mut Scope,
        quiet: bool,
    ) -> ChainValue {
        let ExpressionKind::Property {
            target,
            name,
            nullsafe,
        } = &expression.kind
        else {
            return ChainValue::plain(ValueType::Mixed);
        };
        let mut chain = self.chain_expression(target, scope, quiet);
        let ty = chain.ty.clone();
        if ty == ValueType::Never {
            return chain;
        }
        if self.level >= 2 && *nullsafe && ty == ValueType::Null {
            if self.level >= 8 {
                chain.skip_null(scope);
                return chain;
            }
            if let MemberName::Identifier(property) = name {
                self.property_receiver(&ty, &property.name, expression.span, quiet);
            }
            return chain;
        }
        let guarded = match name {
            MemberName::Identifier(name) if !scope.known.is_empty() => {
                crate::scope::Place::of(target).is_some_and(|receiver| {
                    scope
                        .known
                        .contains(&Symbol::Property(receiver, name.name.clone()))
                })
            }
            _ => false,
        };
        if *nullsafe {
            self.check_nullsafe("property access", target, name, &ty, scope, expression.span);
        }
        let receiver = if self.level >= 8 && *nullsafe {
            chain.skip_null(scope);
            chain.ty.clone()
        } else {
            ty.clone()
        };
        let result = self.property(
            &receiver,
            name,
            scope,
            expression.span,
            quiet || guarded,
            Access::object(target),
        );
        chain.ty = if (4..8).contains(&self.level) && *nullsafe && !ty.excludes_null() {
            result.join(&ValueType::Null)
        } else {
            result
        };
        chain
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
        if self.level >= 2
            && let (Some(class), MemberName::Identifier(member)) = (&class, name)
            && member.name.eq_ignore_ascii_case("class")
        {
            return ValueType::ClassString(std::sync::Arc::new(ValueType::Class(class.clone())));
        }
        if let (Some(class), MemberName::Identifier(name)) = (class, name)
            && !name.name.eq_ignore_ascii_case("class")
            && !scope
                .known
                .contains(&Symbol::Constant(Symbol::constant_name(&format!(
                    "{class}::{}",
                    name.name
                ))))
        {
            if let Some(constant) = self.index.constant(&class, &name.name) {
                return self.relative_type(constant.ty.clone(), &class);
            }
            if let Some(item) =
                self.omitted_builtin_member(ItemKind::ClassConstant, &class, &name.name)
            {
                self.report(Finding::NotSupported(item), name.span);
            } else if self
                .index
                .class(&class)
                .is_some_and(|info| info.kind != ClassKind::Trait)
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
