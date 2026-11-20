//! `str_replace('a', 'b', $text)` returns a string for a string subject.
//! Array subjects keep their keys. Regex replacement can also fail with null.

use crate::types::{Collection, CollectionKind, ShapeKind, ValueType as T};
use std::sync::Arc;

pub fn subject_position(name: &str) -> Option<usize> {
    match name {
        "str_replace"
        | "str_ireplace"
        | "preg_replace"
        | "preg_replace_callback"
        | "preg_filter" => Some(2),
        "preg_replace_callback_array" => Some(1),
        "mb_convert_encoding" | "substr_replace" => Some(0),
        _ => None,
    }
}

pub fn result(name: &str, arguments: &[T]) -> Option<T> {
    let subject = arguments.get(subject_position(name)?)?;
    let preg = name.starts_with("preg_");
    let arms = match subject {
        T::Union(arms) => arms.as_ref(),
        subject => std::slice::from_ref(subject),
    };
    let mut result = T::Never;
    for arm in arms {
        let item = match arm {
            T::String | T::StringLiteral(_) | T::StringRefinement(_) | T::ClassString(_) => {
                if preg {
                    T::String.join(&T::Null)
                } else {
                    T::String
                }
            }
            _ if name == "mb_convert_encoding" => return None,
            T::Null | T::Bool | T::BoolLiteral(_) => continue,
            T::Array => T::Array,
            T::Collection(collection) if collection.kind != CollectionKind::Iterable => {
                T::Collection(Arc::new(Collection {
                    kind: collection.kind,
                    key: collection.key.clone(),
                    value: T::String,
                    non_empty: collection.non_empty && !preg,
                }))
            }
            T::Shape(shape) if shape.kind != ShapeKind::Object => {
                let mut shape = (**shape).clone();
                for field in &mut shape.fields {
                    field.value = T::String;
                    field.optional |= preg;
                }
                if let Some(remainder) = &mut shape.remainder {
                    remainder.value = T::String;
                }
                T::Shape(Arc::new(shape))
            }
            _ => return None,
        };
        result = result.join(&item);
    }
    if result == T::Never {
        return None;
    }
    if name == "mb_convert_encoding" && !arguments.get(2).is_none_or(single_encoding) {
        result = result.join(&T::BoolLiteral(false));
    }
    Some(result)
}

fn single_encoding(ty: &T) -> bool {
    match ty {
        T::Null => true,
        T::StringLiteral(bytes) => std::str::from_utf8(bytes)
            .is_ok_and(|name| !name.contains(',') && !name.trim().eq_ignore_ascii_case("auto")),
        _ => false,
    }
}
