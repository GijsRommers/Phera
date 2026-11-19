//! Atomic expression facts: literals have scalar types and `$x` reads its scope.
//! Uncertain facts become mixed rather than producing a guessed diagnostic.

use super::*;
use phera_source::deprecations::{ItemKind, omitted};

impl Analyzer<'_> {
    pub fn atoms(&mut self, expression: &Expression, scope: &mut Scope, quiet: bool) -> ValueType {
        match &expression.kind {
            ExpressionKind::Literal(Literal::Int(value)) if self.level >= 2 => {
                ValueType::IntLiteral(*value)
            }
            ExpressionKind::Literal(Literal::Bool(value)) if self.level >= 2 => {
                ValueType::BoolLiteral(*value)
            }
            ExpressionKind::Literal(Literal::String(value)) if self.level >= 2 => {
                ValueType::StringLiteral(value.as_slice().into())
            }
            ExpressionKind::Literal(literal) => match literal {
                Literal::Null => ValueType::Null,
                Literal::Bool(_) => ValueType::Bool,
                Literal::Int(_) => ValueType::Int,
                Literal::Float(_) => ValueType::Float,
                Literal::String(_) => ValueType::String,
            },
            ExpressionKind::Variable(Variable::Named(name)) => {
                self.read(name, scope, expression.span, quiet)
            }
            ExpressionKind::Variable(Variable::Dynamic(name)) => {
                if self.expression(name, scope, false) == ValueType::Never {
                    ValueType::Never
                } else {
                    ValueType::Mixed
                }
            }
            ExpressionKind::Name(name) => {
                self.constant_name(name, scope);
                self.context
                    .names
                    .resolve(name, UseKind::Constant)
                    .iter()
                    .find_map(|name| self.index.constants.get(name))
                    .map_or(ValueType::Mixed, |c| c.ty.clone())
            }
            ExpressionKind::MagicConstant(MagicConstant::Line) => ValueType::Int,
            ExpressionKind::MagicConstant(_) => ValueType::String,
            _ => ValueType::Mixed,
        }
    }
    pub fn constant_name(&mut self, name: &Name, scope: &Scope) {
        if matches!(
            name.text.to_ascii_lowercase().as_str(),
            "true" | "false" | "null"
        ) {
            return;
        }
        let candidates = self.context.names.resolve(name, UseKind::Constant);
        if !candidates.iter().any(|n| {
            self.index.constants.contains_key(n)
                || self
                    .context
                    .known
                    .contains(&Symbol::Constant(Symbol::constant_name(n)))
                || scope
                    .known
                    .contains(&Symbol::Constant(Symbol::constant_name(n)))
        }) {
            if let Some(item) = candidates
                .last()
                .and_then(|name| omitted(ItemKind::Constant, name))
            {
                self.report(Finding::NotSupported(item), name.span);
            } else if name.text.starts_with("__") && name.text.ends_with("__") {
                self.report(Finding::MagicConstant(name.text.clone()), name.span);
            } else {
                self.report(Finding::UnknownConstant(candidates[0].clone()), name.span);
            }
        }
    }
}
