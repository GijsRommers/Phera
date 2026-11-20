//! `$record['other']` is absent from a closed `array{value: int}`.
//! A quiet read in isset or a coalesce is handled by its own rule.

use super::*;
use crate::types::{CollectionKind, ShapeKind};

impl Analyzer<'_> {
    pub(super) fn missing_array_offset(
        &mut self,
        array: &ValueType,
        key: &ValueType,
        expression: &Expression,
        scope: &Scope,
        quiet: bool,
    ) {
        if self.level < 3 || quiet {
            return;
        }
        let absent = match array {
            ValueType::Shape(shape)
                if shape.kind != ShapeKind::Object && shape.remainder.is_none() =>
            {
                // Before loop widening is enabled, this shape can still be
                // the initializer from a previous iteration.
                if self.level == 3
                    && self
                        .controls
                        .iter()
                        .any(|frame| frame.kind == super::control::ControlKind::Loop)
                {
                    return;
                }
                super::arrays::shape_key(key)
                    .is_some_and(|key| shape.fields.iter().all(|field| field.key != key))
            }
            ValueType::Collection(collection) if collection.kind == CollectionKind::List => {
                *key == ValueType::Float
            }
            _ => false,
        };
        let nullable_class_key = self.level >= 7 && nullable_class_key(array, key);
        let perhaps_absent =
            nullable_class_key || self.level >= 7 && possible_missing_offset(array, key);
        if (absent || perhaps_absent) && !scope.facts.has_expression(expression) {
            let verb = if absent { "does" } else { "might" };
            let key = if nullable_class_key {
                super::member_checks::receiver_name(key)
            } else {
                key.to_string()
            };
            self.diagnose(
                "offsetAccess.notFound",
                expression.span,
                format!("Offset {key} {verb} not exist on {array}."),
            );
        }
    }
}

fn nullable_class_key(array: &ValueType, key: &ValueType) -> bool {
    matches!(array, ValueType::Collection(collection) if
        collection.kind == CollectionKind::Array && matches!(collection.key, ValueType::ClassString(_)))
        && matches!(key, ValueType::Union(arms) if
            arms.contains(&ValueType::Null) && arms.iter().any(|ty| *ty == ValueType::String))
}

fn possible_missing_offset(array: &ValueType, key: &ValueType) -> bool {
    let Some(key) = super::arrays::shape_key(key) else {
        return false;
    };
    let alternatives = match array {
        ValueType::Union(arms) => arms.as_ref(),
        other => std::slice::from_ref(other),
    };
    let mut may_exist = false;
    let mut may_be_missing = false;
    for alternative in alternatives {
        let ValueType::Shape(shape) = alternative else {
            return false;
        };
        if shape.kind == ShapeKind::Object || shape.remainder.is_some() {
            return false;
        }
        if let Some(field) = shape.fields.iter().find(|field| field.key == key) {
            may_exist = true;
            may_be_missing |= field.optional;
        } else {
            may_be_missing = true;
        }
    }
    may_exist && may_be_missing
}
