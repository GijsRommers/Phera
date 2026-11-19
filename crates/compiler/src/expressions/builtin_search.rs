//! PHP can resolve the haystack before running `in_array($needle, ['GET', 'POST'], true)`.

use phera_ast::{Argument, Expression, ExpressionKind, Literal};
use phera_bytecode::Constant;

pub(super) struct LiteralSearch<'a> {
    pub(super) needle: &'a Expression,
    pub(super) haystack: Vec<Constant>,
    pub(super) strict: bool,
}

impl LiteralSearch<'_> {
    /// An integer or string constant can use the same comparison at compile time.
    pub(super) fn folded(&self) -> Option<bool> {
        match &self.needle.kind {
            ExpressionKind::Literal(Literal::Int(needle)) if self.strict => Some(
                self.haystack
                    .iter()
                    .any(|value| matches!(value, Constant::Int(value) if value == needle)),
            ),
            ExpressionKind::Literal(Literal::String(needle)) => Some(
                self.haystack
                    .iter()
                    .any(|value| matches!(value, Constant::String(value) if value == needle)),
            ),
            _ => None,
        }
    }
}

pub(super) fn literal_search(arguments: &[Argument]) -> Option<LiteralSearch<'_>> {
    let (needle, haystack, strict) = match arguments {
        [needle, haystack] => (needle, haystack, false),
        [needle, haystack, strict] => {
            let ExpressionKind::Literal(value) = &strict.value.kind else {
                return None;
            };
            let strict = match value {
                Literal::Null => false,
                Literal::Bool(value) => *value,
                Literal::Int(value) => *value != 0,
                Literal::Float(value) => *value != 0.0,
                Literal::String(value) => !value.is_empty() && value.as_slice() != b"0",
            };
            (needle, haystack, strict)
        }
        _ => return None,
    };
    let ExpressionKind::Array(items) = &haystack.value.kind else {
        return None;
    };
    let haystack = items
        .iter()
        .map(|item| {
            if item.key.is_some() || item.by_reference || item.spread {
                return None;
            }
            match &item.value.kind {
                ExpressionKind::Literal(Literal::Int(value)) if strict => {
                    Some(Constant::Int(*value))
                }
                ExpressionKind::Literal(Literal::String(value))
                    if strict || !phera_value::is_numeric(value) =>
                {
                    Some(Constant::String(value.clone()))
                }
                _ => None,
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(LiteralSearch {
        needle: &needle.value,
        haystack,
        strict,
    })
}
