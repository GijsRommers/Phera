//! `['user' => new User]['user']` preserves the `User` at the known offset.
//! For partially known arrays, retain a common value type without guessing a key.

use super::*;
use crate::types::{CollectionKind, Field, Shape, ShapeKey, ShapeKind};
use std::sync::Arc;

impl Analyzer<'_> {
    pub fn array_literal(&mut self, items: &[ArrayItem], scope: &mut Scope) -> ValueType {
        // Large literals still evaluate every element. Limit the precise shape
        // so repeatedly joining a scope with a large lookup table stays cheap.
        let mut fields = (self.level >= 2 && items.len() <= 32).then(Vec::new);
        let mut next_key = Some(0i64);
        for item in items {
            let key = if let Some(key) = &item.key {
                let ty = self.expression(key, scope, false);
                if ty == ValueType::Never {
                    return ty;
                }
                shape_key(&ty)
            } else {
                next_key.map(ShapeKey::Integer)
            };
            if item.by_reference {
                self.ensure_reference(&item.value, scope);
            }
            let value = self.expression(&item.value, scope, false);
            if value == ValueType::Never {
                return value;
            }
            if item.spread || item.by_reference || key.is_none() {
                fields = None;
            }
            if let (Some(fields), Some(key)) = (&mut fields, key) {
                if let ShapeKey::Integer(index) = key {
                    // Since PHP 8.3 a negative greatest integer key can
                    // determine where the next automatic index starts.
                    if !fields
                        .iter()
                        .any(|field: &Field| matches!(field.key, ShapeKey::Integer(_)))
                        || next_key.is_some_and(|next| index >= next)
                    {
                        next_key = index.checked_add(1);
                    }
                }
                if let Some(field) = fields.iter_mut().find(|field| field.key == key) {
                    field.value = value;
                } else {
                    fields.push(Field {
                        key,
                        value,
                        optional: false,
                    });
                }
            }
        }
        fields.map_or(ValueType::Array, |fields| {
            let list = fields
                .iter()
                .enumerate()
                .all(|(i, field)| field.key == ShapeKey::Integer(i as i64));
            ValueType::Shape(Arc::new(Shape {
                fields,
                remainder: None,
                kind: if list {
                    ShapeKind::List
                } else {
                    ShapeKind::Array
                },
            }))
        })
    }
}

pub fn offset_type(target: &ValueType, key: &ValueType) -> ValueType {
    let key = shape_key(key);
    let mut pending = vec![target];
    let mut result = ValueType::Never;
    while let Some(ty) = pending.pop() {
        let value = match ty {
            ValueType::Shape(shape) if shape.kind != ShapeKind::Object => {
                if let Some(key) = &key {
                    shape
                        .fields
                        .iter()
                        .find(|field| &field.key == key)
                        .map_or_else(
                            || {
                                shape
                                    .remainder
                                    .as_ref()
                                    .map_or(ValueType::Mixed, |rest| rest.value.clone())
                            },
                            |field| field.value.clone(),
                        )
                } else {
                    let remainder = shape
                        .remainder
                        .as_ref()
                        .map_or(ValueType::Never, |rest| rest.value.clone());
                    shape
                        .fields
                        .iter()
                        .fold(remainder, |values, field| values.join(&field.value))
                }
            }
            ValueType::Collection(collection) => collection.value.clone(),
            ValueType::String | ValueType::StringLiteral(_) | ValueType::StringRefinement(_) => {
                ValueType::String
            }
            ValueType::Union(types) => {
                pending.extend(types.iter());
                continue;
            }
            ValueType::Template(template) => {
                pending.push(&template.bound);
                continue;
            }
            _ => ValueType::Mixed,
        };
        result = result.join(&value);
    }
    if result == ValueType::Never {
        ValueType::Mixed
    } else {
        result
    }
}

fn shape_key(ty: &ValueType) -> Option<ShapeKey> {
    match ty {
        ValueType::IntLiteral(value) => Some(ShapeKey::Integer(*value)),
        ValueType::BoolLiteral(value) => Some(ShapeKey::Integer(i64::from(*value))),
        ValueType::Null => Some(ShapeKey::String("".into())),
        ValueType::StringLiteral(value) => {
            let value = std::str::from_utf8(value).ok()?;
            Some(match value.parse::<i64>() {
                Ok(integer) if integer.to_string() == value => ShapeKey::Integer(integer),
                _ => ShapeKey::String(value.into()),
            })
        }
        _ => None,
    }
}

pub fn iteration_types(ty: &ValueType) -> (ValueType, ValueType) {
    let mut keys = ValueType::Never;
    let mut values = ValueType::Never;
    let mut pending = vec![ty];
    while let Some(ty) = pending.pop() {
        match ty {
            ValueType::Collection(collection) => {
                keys = keys.join(&collection.key);
                values = values.join(&collection.value);
            }
            ValueType::Shape(shape) if shape.kind != ShapeKind::Object => {
                for field in &shape.fields {
                    let key = match &field.key {
                        ShapeKey::Integer(value) => ValueType::IntLiteral(*value),
                        ShapeKey::String(value) => {
                            ValueType::StringLiteral(value.as_bytes().into())
                        }
                    };
                    keys = keys.join(&key);
                    values = values.join(&field.value);
                }
                if let Some(rest) = &shape.remainder {
                    keys = keys.join(&rest.key);
                    values = values.join(&rest.value);
                }
            }
            ValueType::Union(arms) => pending.extend(arms.iter()),
            ValueType::Template(template) => pending.push(&template.bound),
            _ => return (ValueType::Mixed, ValueType::Mixed),
        }
    }
    if keys == ValueType::Never {
        (ValueType::Mixed, ValueType::Mixed)
    } else {
        (keys, values)
    }
}

pub(super) fn nonempty_array(ty: &ValueType) -> bool {
    match ty {
        ValueType::Collection(collection) => {
            collection.kind != CollectionKind::Iterable && collection.non_empty
        }
        ValueType::Shape(shape) => {
            shape.kind != ShapeKind::Object && shape.fields.iter().any(|field| !field.optional)
        }
        _ => false,
    }
}

pub(super) fn when_truthy(ty: ValueType) -> ValueType {
    match ty {
        ValueType::Collection(collection)
            if collection.kind != CollectionKind::Iterable && !collection.non_empty =>
        {
            let mut collection = (*collection).clone();
            collection.non_empty = true;
            ValueType::Collection(std::sync::Arc::new(collection))
        }
        _ => ty,
    }
}
