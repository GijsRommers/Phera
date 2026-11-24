//! `array_merge(...$lists)` preserves the element types of the unpacked arrays.

use crate::types::{Collection, CollectionKind, ShapeKind, ValueType as T};
use phera_ast::Argument;
use std::sync::Arc;

pub(super) fn result(arguments: &[Argument], types: &[T]) -> Option<T> {
    if arguments.len() != types.len() || arguments.iter().any(|argument| argument.name.is_some()) {
        return None;
    }
    let mut keys = T::Never;
    let mut values = T::Never;
    let mut lists = true;
    for (argument, ty) in arguments.iter().zip(types) {
        let unpacked;
        let ty = if argument.spread {
            unpacked = super::arrays::iteration_types(ty).1;
            if unpacked.is_mixed() {
                return None;
            }
            &unpacked
        } else {
            ty
        };
        let (key, value) = if *ty == T::Array {
            (T::Int.join(&T::String), T::Mixed)
        } else {
            super::arrays::iteration_types(ty)
        };
        keys = keys.join(&key);
        values = values.join(&value);
        lists &= matches!(ty, T::Collection(items) if items.kind == CollectionKind::List)
            || matches!(ty, T::Shape(shape) if shape.kind == ShapeKind::List);
    }
    Some(T::Collection(Arc::new(Collection {
        kind: if lists {
            CollectionKind::List
        } else {
            CollectionKind::Array
        },
        key: if lists { T::Int } else { keys },
        value: values,
        non_empty: false,
    })))
}
