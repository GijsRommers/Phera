//! `array_sum($counts)` can use the integer elements of `list<int>`.
//! Keep the possible promotion to float when the sum is too large.

use crate::types::{CollectionKind, ShapeKind, ValueType as T};

pub(super) fn result(array: &T) -> Option<T> {
    let items = match array {
        T::Collection(items) if items.kind != CollectionKind::Iterable => items.value.clone(),
        T::Shape(shape) if shape.kind != ShapeKind::Object => {
            if shape.fields.is_empty() && shape.remainder.is_none() {
                return Some(T::IntLiteral(0));
            }
            super::arrays::iteration_types(array).1
        }
        _ => return None,
    };
    let variants = match &items {
        T::Union(variants) | T::BenevolentUnion(variants) => variants.as_ref(),
        ty => std::slice::from_ref(ty),
    };
    if variants
        .iter()
        .all(|ty| matches!(ty, T::Int | T::IntLiteral(_) | T::IntRange(..)))
    {
        Some(T::OverflowingInt)
    } else if variants.iter().all(|ty| *ty == T::Float) {
        Some(T::Float.join(&T::IntLiteral(0)))
    } else {
        None
    }
}
