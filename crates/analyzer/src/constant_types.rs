//! Constant value types: `const CALLBACK = static function (int $x) {};` is callable.
//! Infer syntax and known declarations only; initializers are never evaluated.

use crate::{
    index::{Index, Location, Signature},
    names::Names,
    types::ValueType,
};
use phera_ast::*;

pub fn infer(
    expression: &Expression,
    names: &Names,
    location: Location,
    index: &Index,
) -> ValueType {
    match &expression.kind {
        ExpressionKind::Literal(literal) => match literal {
            Literal::Null => ValueType::Null,
            Literal::Bool(_) => ValueType::Bool,
            Literal::Int(_) => ValueType::Int,
            Literal::Float(_) => ValueType::Float,
            Literal::String(_) => ValueType::String,
        },
        ExpressionKind::Array(_) => ValueType::Array,
        ExpressionKind::Closure(closure) => {
            let mut signature = Signature::new(
                &closure.parameters,
                closure.return_type.as_ref(),
                names,
                location,
            );
            signature.global_effects = crate::global_effects::of(&closure.body);
            ValueType::Callable(std::sync::Arc::new(signature))
        }
        ExpressionKind::ArrowFunction(arrow) => {
            let mut signature = Signature::new(
                &arrow.parameters,
                arrow.return_type.as_ref(),
                names,
                location,
            );
            signature.global_effects = crate::global_effects::expression(&arrow.body);
            ValueType::Callable(std::sync::Arc::new(signature))
        }
        ExpressionKind::Call {
            callee,
            arguments: Arguments::FirstClassCallable,
        } => {
            if let ExpressionKind::Name(name) = &callee.kind
                && callee.span == name.span
            {
                names
                    .resolve(name, UseKind::Function)
                    .iter()
                    .find_map(|name| index.functions.get(&name.to_ascii_lowercase()))
                    .map_or(ValueType::Mixed, |s| {
                        ValueType::Callable(std::sync::Arc::new(s.clone()))
                    })
            } else {
                ValueType::Mixed
            }
        }
        ExpressionKind::Name(name) => names
            .resolve(name, UseKind::Constant)
            .iter()
            .find_map(|name| index.constants.get(name))
            .map_or(ValueType::Mixed, |c| c.ty.clone()),
        _ => ValueType::Mixed,
    }
}
