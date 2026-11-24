//! `array_key_exists('value', $row)` proves the key exists, even if it is null.

use super::*;
use crate::{
    scope::{AccessStep, Place},
    types::ShapeKey,
};

impl Analyzer<'_> {
    pub(super) fn refine_array_key_exists(&self, arguments: &[Argument], scope: &mut Scope) {
        let [first, second] = arguments else { return };
        if first.spread || second.spread {
            return;
        }
        let (key, array) = if first.name.as_ref().is_some_and(|arg| arg.name == "array") {
            (second, first)
        } else {
            (first, second)
        };
        let (Some(ty), Some(mut place)) = (
            crate::constant_types::literal(&key.value),
            Place::of(&array.value),
        ) else {
            return;
        };
        let Some(key) = super::arrays::shape_key(&ty) else {
            return;
        };
        let value = super::arrays::offset_type(&self.place_type(&place, scope), &ty);
        let offset = match key {
            ShapeKey::Integer(key) => AccessStep::IntegerOffset(key),
            ShapeKey::String(key) => AccessStep::StringOffset(key.as_bytes().to_vec()),
        };
        place.members.push(offset);
        scope.facts.record(place, value, None);
    }
}
