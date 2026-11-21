//! Declared property positions bypass hashing mangled keys on ordinary object access.

use crate::classes::Class;
use phera_value::{ArrayKey, Object, Value};

/// The declared slot of `key` in the object's class: the property's place
/// in the class's layout (`Class::layout`), which is also its place in
/// `Class::defaults`.
fn position(object: &Object, key: &ArrayKey) -> Option<usize> {
    let class = object.class.as_any().downcast_ref::<Class>()?;
    class.layout.as_ref()?.slot(key)
}

/// The value of the property stored under `key`.
pub(crate) fn read(object: &Object, key: &ArrayKey) -> Option<Value> {
    read_at(object, key, position(object, key))
}

/// The same, with the slot a site or a `ReflectionProperty` remembered.
pub(crate) fn read_at(object: &Object, key: &ArrayKey, slot: Option<usize>) -> Option<Value> {
    object
        .with_properties(|properties| properties.get_at(key, slot).cloned())
        .flatten()
}

/// Move an array out of a property so nested append retains its unique owner.
pub(crate) fn take(object: &Object, key: &ArrayKey) -> Value {
    let position = position(object, key);
    object
        .update(|properties| {
            properties
                .get_mut_at(key, position)
                .map(std::mem::take)
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Writes the property stored under `key`, creating it if needed.
pub(crate) fn write(object: &Object, key: ArrayKey, value: Value) {
    let slot = position(object, &key);
    write_at(object, key, slot, value);
}

/// The same, with a remembered slot.
pub(crate) fn write_at(object: &Object, key: ArrayKey, slot: Option<usize>, value: Value) {
    object.update(|properties| properties.insert_at(key, slot, value));
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
        .update(|properties| match properties.get_mut_at(key, slot) {
            Some(Value::Reference(_)) => false,
            Some(destination) => {
                *destination = value.clone();
                true
            }
            None if may_initialize() => {
                properties.insert_at(key.clone(), slot, value.clone());
                true
            }
            None => false,
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
            let Some(Value::Array(array)) = properties.get_mut_at(key, slot) else {
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
    (keys, place): (&[ArrayKey], crate::element_writes::Target),
    value: Value,
) -> Option<Option<Value>> {
    object
        .update(|properties| {
            let Some(Value::Array(array)) = properties.get_mut_at(key, slot) else {
                return None;
            };
            if !crate::element_writes::writable(array, keys, place) {
                return None;
            }
            crate::element_writes::write(array, keys, place, value)
        })
        .flatten()
}
