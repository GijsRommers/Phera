//! `array_values($users)` produces `list<User>` and `count([])` produces `0`.
//! Refine php-src signatures from evaluated arguments without executing PHP.

use super::*;
use crate::types::{Collection, CollectionKind, Field, Shape, ShapeKey, ShapeKind};
use std::sync::Arc;

pub fn takes_facts(label: &str) -> bool {
    matches!(
        label,
        "array_values()"
            | "array_keys()"
            | "array_map()"
            | "array_filter()"
            | "array_pop()"
            | "array_shift()"
            | "array_column()"
            | "array_search()"
            | "array_slice()"
            | "array_reverse()"
            | "array_unique()"
            | "array_diff()"
            | "array_merge()"
            | "count()"
            | "sizeof()"
            | "strlen()"
            | "get_class()"
            | "preg_match()"
            | "preg_match_all()"
    )
}

impl Analyzer<'_> {
    pub fn builtin_result(
        &mut self,
        name: &str,
        arguments: &Arguments,
        types: &[ValueType],
        scope: &mut Scope,
    ) -> Option<ValueType> {
        use ValueType as T;
        let Arguments::List(arguments) = arguments else {
            return None;
        };
        if arguments
            .iter()
            .any(|argument| argument.spread || argument.name.is_some())
        {
            return None;
        }
        let first = types.first();
        let array_parts = || first.map_or((T::Mixed, T::Mixed), super::arrays::iteration_types);
        Some(match name {
            "array_values" => list(array_parts().1),
            "array_keys" => list(array_parts().0),
            "array_map" => {
                let (callback, arrays) = types.split_first()?;
                let array = arrays.first()?;
                if arrays.len() == 1 && *callback == T::Null {
                    return Some(array.clone());
                }
                let value = self
                    .callable_signature(callback.clone())
                    .map_or(T::Mixed, |signature| signature.returns);
                let (key, _) = super::arrays::iteration_types(array);
                if arrays.len() > 1 || is_list(array) {
                    list(value)
                } else {
                    collection(key, value)
                }
            }
            "array_filter" | "array_unique" | "array_diff" => {
                let (key, value) = array_parts();
                collection(key, value)
            }
            "array_slice" | "array_reverse" => {
                let first = first?;
                let (key, value) = array_parts();
                let flag = if name == "array_slice" { 3 } else { 1 };
                if is_list(first)
                    && types
                        .get(flag)
                        .is_none_or(|flag| *flag == T::BoolLiteral(false))
                {
                    list(value)
                } else {
                    collection(key, value)
                }
            }
            "array_merge" => {
                let (key, value) = types
                    .iter()
                    .map(super::arrays::iteration_types)
                    .fold((T::Never, T::Never), |(keys, values), (key, value)| {
                        (keys.join(&key), values.join(&value))
                    });
                if types.iter().all(is_list) {
                    list(value)
                } else {
                    collection(key, value)
                }
            }
            "array_column" => {
                let (_, row) = array_parts();
                let column = types.get(1)?;
                let value = if *column == T::Null {
                    row.clone()
                } else {
                    super::arrays::offset_type(&row, column)
                };
                if let Some(index) = types.get(2).filter(|ty| **ty != T::Null) {
                    collection(super::arrays::offset_type(&row, index), value)
                } else {
                    list(value)
                }
            }
            "array_pop" | "array_shift" => array_parts().1.join(&T::Null),
            "array_search" => {
                let (key, _) = super::arrays::iteration_types(types.get(1)?);
                key.join(&T::BoolLiteral(false))
            }
            "count" | "sizeof" if types.len() <= 1 => match first {
                Some(T::Shape(shape))
                    if shape.remainder.is_none()
                        && shape.kind != ShapeKind::Object
                        && shape.fields.iter().all(|field| !field.optional) =>
                {
                    T::IntLiteral(shape.fields.len() as i64)
                }
                _ => T::IntRange(0, i64::MAX),
            },
            "strlen" => match first {
                Some(T::StringLiteral(bytes)) => T::IntLiteral(bytes.len() as i64),
                _ => T::IntRange(0, i64::MAX),
            },
            "get_class" => match first {
                Some(ty) if ty.class_name().is_some() => T::ClassString(Arc::new(ty.clone())),
                _ => T::ClassString(Arc::new(T::Object)),
            },
            "preg_match" | "preg_match_all" => {
                let multiple = name == "preg_match_all";
                if let Some(argument) = arguments.get(2)
                    && let ExpressionKind::Variable(Variable::Named(name)) = &argument.value.kind
                {
                    let flags = types.get(3).cloned().unwrap_or(T::IntLiteral(0));
                    scope.define(name, match_result(&flags, multiple));
                }
                if multiple {
                    T::IntRange(0, i64::MAX)
                } else {
                    T::IntRange(0, 1)
                }
                .join(&T::BoolLiteral(false))
            }
            _ => return None,
        })
    }
}

fn match_result(flags: &ValueType, multiple: bool) -> ValueType {
    use ValueType as T;
    let T::IntLiteral(flags) = flags else {
        return T::Array;
    };
    let value = if flags & 512 == 0 {
        T::String
    } else {
        T::String.join(&T::Null)
    };
    let value = if flags & 256 == 0 {
        value
    } else {
        T::Shape(Arc::new(Shape {
            fields: vec![
                Field {
                    key: ShapeKey::Integer(0),
                    value,
                    optional: false,
                },
                Field {
                    key: ShapeKey::Integer(1),
                    value: T::Int,
                    optional: false,
                },
            ],
            remainder: None,
            kind: ShapeKind::List,
        }))
    };
    let keys = T::Int.join(&T::String);
    if multiple {
        if flags & 2 == 0 {
            collection(keys, list(value))
        } else {
            list(collection(keys, value))
        }
    } else {
        collection(keys, value)
    }
}

fn is_list(ty: &ValueType) -> bool {
    matches!(ty, ValueType::Collection(collection) if collection.kind == CollectionKind::List)
        || matches!(ty, ValueType::Shape(shape) if shape.kind == ShapeKind::List)
}

fn list(value: ValueType) -> ValueType {
    ValueType::Collection(Arc::new(Collection {
        kind: CollectionKind::List,
        key: ValueType::Int,
        value,
        non_empty: false,
    }))
}

fn collection(key: ValueType, value: ValueType) -> ValueType {
    ValueType::Collection(Arc::new(Collection {
        kind: CollectionKind::Array,
        key,
        value,
        non_empty: false,
    }))
}
