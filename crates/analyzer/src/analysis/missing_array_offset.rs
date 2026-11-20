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
        let missing = match array {
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
        if missing && !scope.facts.has_expression(expression) {
            self.diagnose(
                "offsetAccess.notFound",
                expression.span,
                format!("Offset {key} does not exist on {array}."),
            );
        }
    }
}
