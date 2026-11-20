//! `$n === '3'` is false for an integer, regardless of its value.
//! Ordering uses interval endpoints only when both operands are integers.

use super::*;
use crate::constant_types::int_bounds;
use BinaryOperator as Op;
use ValueType as T;

impl Analyzer<'_> {
    pub fn constant_comparison(
        &mut self,
        operator: Op,
        left: (&Expression, &T),
        right: (&Expression, &T),
        scope: &Scope,
        span: Span,
    ) -> Option<T> {
        let (kind, symbol, style) = match operator {
            Op::Identical => ("identical", "===", "Strict comparison using"),
            Op::NotIdentical => ("notIdentical", "!==", "Strict comparison using"),
            Op::Equal => ("equal", "==", "Loose comparison using"),
            Op::NotEqual => ("notEqual", "!=", "Loose comparison using"),
            Op::Less => ("smaller", "<", "Comparison operation"),
            Op::LessEqual => ("smallerOrEqual", "<=", "Comparison operation"),
            Op::Greater => ("greater", ">", "Comparison operation"),
            Op::GreaterEqual => ("greaterOrEqual", ">=", "Comparison operation"),
            _ => return None,
        };
        if matches!(operator, Op::Identical | Op::NotIdentical)
            && (left.1 == &T::Null && self.may_read_implicit_null(right.0, scope)
                || right.1 == &T::Null && self.may_read_implicit_null(left.0, scope))
        {
            return None;
        }
        let result = uniform_pairs(left.1, right.1, |a, b| compare(operator, a, b))?;
        let provenance_left = self.narrowed_at(left.0, scope);
        let provenance_right = self.narrowed_at(right.0, scope);
        let provenance = if provenance_left.is_empty() {
            provenance_right
        } else {
            provenance_left
        };
        let (left, right) = (left.1, right.1);
        let message = if matches!(
            operator,
            Op::Less | Op::LessEqual | Op::Greater | Op::GreaterEqual
        ) {
            format!(
                "{style} \"{symbol}\" between {left} and {right} is always {result}.{provenance}"
            )
        } else {
            format!(
                "{style} {symbol} between {left} and {right} will always evaluate to {result}.{provenance}"
            )
        };
        self.constant_result(kind, span, result, message);
        Some(T::BoolLiteral(result))
    }
}

fn compare(operator: Op, left: &T, right: &T) -> Option<bool> {
    if let (Some((a, b)), Some((c, d))) = (int_bounds(left), int_bounds(right)) {
        return match operator {
            Op::Identical | Op::Equal if b < c || d < a => Some(false),
            Op::NotIdentical | Op::NotEqual if b < c || d < a => Some(true),
            Op::Identical | Op::Equal if a == b && c == d => Some(a == c),
            Op::NotIdentical | Op::NotEqual if a == b && c == d => Some(a != c),
            Op::Less => certain(b < c, a >= d),
            Op::LessEqual => certain(b <= c, a > d),
            Op::Greater => certain(a > d, b <= c),
            Op::GreaterEqual => certain(a >= d, b < c),
            _ => None,
        };
    }
    let equality = match operator {
        Op::Identical | Op::NotIdentical => identical(left, right),
        Op::Equal | Op::NotEqual => loose_equal(left, right),
        _ => return None,
    }?;
    Some(equality != matches!(operator, Op::NotIdentical | Op::NotEqual))
}

fn certain(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

fn identical(left: &T, right: &T) -> Option<bool> {
    match (left, right) {
        (T::Null | T::Void, T::Null | T::Void) => Some(true),
        (T::BoolLiteral(left), T::BoolLiteral(right)) => Some(left == right),
        (T::StringLiteral(left), T::StringLiteral(right)) => Some(left == right),
        (T::StringLiteral(literal), T::ClassString(_))
        | (T::ClassString(_), T::StringLiteral(literal))
            if literal.is_empty() =>
        {
            Some(false)
        }
        _ if category(left)? != category(right)? => Some(false),
        _ => None,
    }
}

fn loose_equal(left: &T, right: &T) -> Option<bool> {
    match (left, right) {
        (T::BoolLiteral(value), other) | (other, T::BoolLiteral(value)) => {
            Some(*value == super::truth(other)?)
        }
        (T::Null, T::Null) => Some(true),
        (T::Null, T::IntLiteral(value)) | (T::IntLiteral(value), T::Null) => Some(*value == 0),
        (T::Null, T::StringLiteral(value)) | (T::StringLiteral(value), T::Null) => {
            Some(value.is_empty())
        }
        _ => None,
    }
}

#[derive(PartialEq, Eq)]
pub(super) enum Category {
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    Resource,
}

pub(super) fn category(ty: &T) -> Option<Category> {
    Some(match ty {
        T::Null | T::Void => Category::Null,
        T::Bool | T::BoolLiteral(_) => Category::Bool,
        T::Int | T::IntLiteral(_) | T::IntRange(..) => Category::Int,
        T::Float => Category::Float,
        T::String | T::StringLiteral(_) | T::StringRefinement(_) | T::ClassString(_) => {
            Category::String
        }
        T::Array => Category::Array,
        T::Collection(collection) if collection.kind != CollectionKind::Iterable => Category::Array,
        T::Shape(shape) if shape.kind != ShapeKind::Object => Category::Array,
        T::Callable(signature) if signature.closure_object => Category::Object,
        T::Object | T::Class(_) | T::Generic(_) | T::This | T::Shape(_) => Category::Object,
        T::Resource => Category::Resource,
        _ => return None,
    })
}

pub(super) fn uniform_pairs(
    left: &T,
    right: &T,
    mut evaluate: impl FnMut(&T, &T) -> Option<bool>,
) -> Option<bool> {
    let parts = |ty: &'_ T| match ty {
        T::Union(types) => types.len(),
        _ => 1,
    };
    // Wide unions can come from generated signatures. Leave a comparison
    // unknown when proving it would require many combinations.
    if parts(left).saturating_mul(parts(right)) > 64 {
        return None;
    }
    let mut result = None;
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        match (left, right) {
            (T::Union(types), _) => pending.extend(types.iter().map(|ty| (ty, right))),
            (_, T::Union(types)) => pending.extend(types.iter().map(|ty| (left, ty))),
            (T::Never, _) | (_, T::Never) => continue,
            _ => {
                let value = evaluate(left, right)?;
                if result.is_some_and(|existing| existing != value) {
                    return None;
                }
                result = Some(value);
            }
        }
    }
    result
}
