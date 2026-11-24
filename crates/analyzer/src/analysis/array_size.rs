//! After `if (count($rows) < 2) return;`, two calls to `array_pop($rows)`
//! each have an element to return.

use super::*;
use crate::types::{Collection, CollectionKind, ShapeKind};
use std::sync::Arc;

impl Analyzer<'_> {
    pub(super) fn refine_array_size(
        &self,
        operator: BinaryOperator,
        left: &Expression,
        right: &Expression,
        scope: &mut Scope,
        truth: bool,
    ) {
        let (call, limit, operator) = match (&left.kind, &right.kind) {
            (_, ExpressionKind::Literal(Literal::Int(limit))) => (left, *limit, operator),
            (ExpressionKind::Literal(Literal::Int(limit)), _) => {
                (right, *limit, reversed(operator))
            }
            _ => return,
        };
        let minimum = match (operator, truth) {
            (BinaryOperator::GreaterEqual, true) | (BinaryOperator::Less, false) => limit,
            (BinaryOperator::Greater, true) | (BinaryOperator::LessEqual, false) => {
                limit.saturating_add(1)
            }
            _ => return,
        };
        let Ok(minimum) = usize::try_from(minimum) else {
            return;
        };
        if minimum == 0 {
            return;
        }
        if let ExpressionKind::Call {
            callee,
            arguments: Arguments::List(args),
            ..
        } = &call.kind
            && let ExpressionKind::Name(function) = &callee.kind
            && matches!(
                function.text.to_ascii_lowercase().as_str(),
                "count" | "sizeof"
            )
            && args.len() <= 2
            && args.iter().all(|arg| !arg.spread && arg.name.is_none())
            && let Some(arg) = args.first()
            && let ExpressionKind::Variable(Variable::Named(name)) = &arg.value.kind
            && args.get(1).is_none_or(|mode| {
                matches!(mode.value.kind, ExpressionKind::Literal(Literal::Int(0)))
            })
            && self
                .builtin_predicate(function)
                .is_some_and(|name| matches!(name.as_str(), "count" | "sizeof"))
        {
            scope.facts.remember_array_size(name, minimum);
        }
    }
}

fn reversed(operator: BinaryOperator) -> BinaryOperator {
    match operator {
        BinaryOperator::Less => BinaryOperator::Greater,
        BinaryOperator::LessEqual => BinaryOperator::GreaterEqual,
        BinaryOperator::Greater => BinaryOperator::Less,
        BinaryOperator::GreaterEqual => BinaryOperator::LessEqual,
        _ => operator,
    }
}

pub(super) struct Removal<'a> {
    name: &'a str,
    collection: Collection,
    minimum: usize,
}

impl<'a> Removal<'a> {
    pub(super) fn from_call(
        function: Option<&str>,
        arguments: &'a Arguments,
        scope: &Scope,
    ) -> Option<Self> {
        if !matches!(function, Some("array_pop" | "array_shift")) || scope.dynamic {
            return None;
        }
        let Arguments::List(arguments) = arguments else {
            return None;
        };
        let [argument] = arguments.as_slice() else {
            return None;
        };
        if argument.spread
            || argument
                .name
                .as_ref()
                .is_some_and(|name| name.name != "array")
        {
            return None;
        }
        let ExpressionKind::Variable(Variable::Named(name)) = &argument.value.kind else {
            return None;
        };
        let minimum = scope.facts.minimum_array_size(name).checked_sub(1)?;
        if scope.aliases.contains_key(name) || scope.external_references.contains(name) {
            return None;
        }
        let variable = scope.get(name);
        let kind = match &variable.ty {
            ValueType::Array => CollectionKind::Array,
            ValueType::Collection(array) if array.kind != CollectionKind::Iterable => array.kind,
            ValueType::Shape(shape) if shape.kind == ShapeKind::List => CollectionKind::List,
            ValueType::Shape(shape) if shape.kind == ShapeKind::Array => CollectionKind::Array,
            _ => return None,
        };
        let (key, value) = super::arrays::iteration_types(&variable.ty);
        Some(Self {
            name,
            collection: Collection {
                kind,
                key,
                value,
                non_empty: minimum > 0,
            },
            minimum,
        })
    }

    pub(super) fn apply(self, scope: &mut Scope) -> ValueType {
        let value = self.collection.value.clone();
        scope.external_references.remove(self.name);
        scope.define(self.name, ValueType::Collection(Arc::new(self.collection)));
        scope.facts.remember_array_size(self.name, self.minimum);
        value
    }
}
