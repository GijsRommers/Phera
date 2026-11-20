//! A live WeakMap entry may hold a PHP reference.
//!
//! ```php
//! $map[$key] = 0;
//! $map[$key] =& $number;
//! $number++; // reading the map entry sees the new value
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    Context, DimensionReferenceMode, NativeError, NativeResult, Object, Value,
};

use super::{Map, key, missing_key};

fn modify(
    map: &Object,
    key: &Rc<Object>,
    change: impl FnOnce(&mut Value) -> Value,
) -> NativeResult {
    crate::native_state::update_present_state(map, |state: &mut Map| state.update(key, change))?
        .flatten()
        .ok_or_else(|| missing_key(key))
}

pub(super) fn current_reference(map: &Object, key: &Rc<Object>) -> NativeResult {
    modify(map, key, |value| {
        if !value.is_reference() {
            *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
        }
        value.clone()
    })
}

pub(super) fn reference(
    _: &mut dyn Context,
    map: &Object,
    offset: Value,
    _: DimensionReferenceMode,
) -> NativeResult {
    current_reference(map, &key(offset, false)?)
}

pub(super) fn bind(
    _: &mut dyn Context,
    map: &Object,
    offset: Value,
    reference: Value,
) -> Result<(), NativeError> {
    modify(map, &key(offset, false)?, |value| {
        *value = reference;
        Value::Null
    })?;
    Ok(())
}
