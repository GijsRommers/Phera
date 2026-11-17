//! Member expression facts: `self::MISSING` checks the indexed class constants.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;

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
                let class = self.class_expression(class, scope);
                if let Variable::Named(name) = name
                    && let Some(class) = class
                {
                    return self.named_property(
                        &class,
                        name,
                        expression.span,
                        quiet,
                        PropertyAccess::Static,
                    );
                }
                if let Variable::Dynamic(name) = name {
                    self.expression(name, scope, false);
                }
                ValueType::Mixed
            }
            ExpressionKind::ClassConstant { class, name } => {
                let class = self.class_expression(class, scope);
                if let MemberName::Expression(name) = name {
                    self.expression(name, scope, false);
                }
                if let (Some(class), MemberName::Identifier(name)) = (class, name)
                    && !name.name.eq_ignore_ascii_case("class")
                    && self.index.class(&class).is_some()
                    && self.index.complete(&class)
                    && !self.index.constant(&class, &name.name)
                {
                    self.report(
                        Finding::UnknownClassConstant(format!("{class}::{}", name.name)),
                        name.span,
                    );
                }
                ValueType::Mixed
            }
            _ => ValueType::Mixed,
        }
    }
}
