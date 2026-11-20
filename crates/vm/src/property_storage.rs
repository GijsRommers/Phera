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
    read_at(object, key, position(object, key))
}

/// Mirrors read with a cached declaration position; the actual key still guards layout changes.
pub(crate) fn read_at(object: &Object, key: &ArrayKey, slot: Option<usize>) -> Option<Value> {
    if let Some(position) = slot
        && let Some(value) = object
            .update(|properties| {
                properties
                    .entry_at(position)
                    .filter(|(stored, _)| *stored == *key)
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
                && stored == *key
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
    let slot = position(object, &key);
    write_at(object, key, slot, value);
}

/// Mirrors write using a guarded cached position, falling back after unset/reinsertion.
pub(crate) fn write_at(object: &Object, key: ArrayKey, slot: Option<usize>, value: Value) {
    object.update(|properties| {
        if let Some(position) = slot
            && let Some((stored, target)) = properties.entry_at_mut(position)
            && stored == key
        {
            *target = value;
            return;
        }
        properties.insert(key, value);
    });
}

/// A cached write may replace a value, but must preserve writable reference cells.
pub(crate) fn write_direct_at(
    object: &Object,
    key: &ArrayKey,
    slot: Option<usize>,
    value: &Value,
    may_initialize: impl FnOnce() -> bool,
) -> bool {
    object
        .update(|properties| {
            let destination = match slot.and_then(|slot| {
                properties
                    .entry_at(slot)
                    .filter(|(stored, _)| stored == key)
                    .map(|_| slot)
            }) {
                Some(slot) => properties.entry_at_mut(slot).map(|(_, value)| value),
                None => properties.get_mut(key),
            };
            match destination {
                Some(Value::Reference(_)) => false,
                Some(destination) => {
                    *destination = value.clone();
                    true
                }
                None if may_initialize() => {
                    properties.insert(key.clone(), value.clone());
                    true
                }
                None => false,
            }
        })
        .unwrap_or(false)
}

/// Mutate a stored array in place. References and absent/nonarray slots return
/// None so the general path retains type, promotion and alias diagnostics.
pub(crate) fn append_at(
    object: &Object,
    key: &ArrayKey,
    slot: Option<usize>,
    value: &Value,
) -> Option<Result<(), phera_extension_api::NativeError>> {
    object
        .update(|properties| {
            let position = slot.filter(|position| {
                properties
                    .entry_at(*position)
                    .is_some_and(|(stored, _)| stored == *key)
            });
            let target = if let Some(position) = position {
                properties.entry_at_mut(position).map(|(_, value)| value)
            } else {
                properties.get_mut(key)
            };
            let Some(Value::Array(array)) = target else {
                return None;
            };
            Some(
                std::rc::Rc::make_mut(array)
                    .push(value.clone())
                    .map(|_| ())
                    .map_err(Into::into),
            )
        })
        .flatten()
}

/// A plain write of an element of the array a property holds (see
/// `crate::element_writes`): what the element held, or `None` with
/// nothing changed when the property is not such an array.
pub(crate) fn write_element_at(
    object: &Object,
    key: &ArrayKey,
    slot: Option<usize>,
    keys: &[ArrayKey],
    value: Value,
) -> Option<Option<Value>> {
    object
        .update(|properties| {
            let position = slot.filter(|position| {
                properties
                    .entry_at(*position)
                    .is_some_and(|(stored, _)| stored == *key)
            });
            let target = if let Some(position) = position {
                properties.entry_at_mut(position).map(|(_, value)| value)
            } else {
                properties.get_mut(key)
            };
            let Some(Value::Array(array)) = target else {
                return None;
            };
            if !crate::element_writes::writable(array, keys) {
                return None;
            }
            crate::element_writes::write(array, keys, value)
        })
        .flatten()
}
