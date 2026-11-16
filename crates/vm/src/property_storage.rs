//! Declared property positions bypass hashing mangled keys on ordinary object access.

use crate::classes::Class;
use phera_value::{ArrayKey, Object, Value};

/// Layout positions are computed by root-first linking, including redeclarations.
fn position(object: &Object, key: &ArrayKey) -> Option<usize> {
    let class = object.class.as_any().downcast_ref::<Class>()?;
    class
        .defaults
        .iter()
        .position(|property| property.key == *key)
}

/// Unset/reinsert can change a position; verify its key before using the fast slot.
pub(crate) fn read(object: &Object, key: &ArrayKey) -> Option<Value> {
    if let Some(position) = position(object, key)
        && let Some(value) = object
            .update(|properties| {
                properties
                    .entry_at(position)
                    .filter(|(stored, _)| *stored == key)
                    .map(|(_, value)| value.clone())
            })
            .flatten()
    {
        return Some(value);
    }
    object.get(key)
}

/// Move an array out of a property so nested append retains its unique owner.
pub(crate) fn take(object: &Object, key: &ArrayKey) -> Value {
    let position = position(object, key);
    object
        .update(|properties| {
            if let Some(position) = position
                && let Some((stored, value)) = properties.entry_at_mut(position)
                && stored == key
            {
                return std::mem::take(value);
            }
            properties
                .get_mut(key)
                .map(std::mem::take)
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Existing declared slots need no key hashing; dynamic/reinserted slots use the map.
pub(crate) fn write(object: &Object, key: ArrayKey, value: Value) {
    let position = position(object, &key);
    object.update(|properties| {
        if let Some(position) = position
            && let Some((stored, target)) = properties.entry_at_mut(position)
            && *stored == key
        {
            *target = value;
            return;
        }
        properties.insert(key, value);
    });
}
