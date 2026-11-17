//! PHP array registration and shared key-retention and list-construction helpers.

use phera_extension_api::{ArrayKey, Registry, Value};

mod build;
mod callback;
mod callback_set_merge;
mod callback_sets;
mod callback_sort;
mod combine;
mod combine_recursive;
mod filter;
mod find;
mod keys;
mod ordering;
mod pointer;
mod range;
mod range_endpoint;
mod range_values;
mod sets;
mod slice;
mod sort;
pub(crate) mod sort_order;
mod sort_order_async;
mod stack;
mod tables;
mod walk;

pub(crate) fn register(registry: &mut Registry) {
    callback_sort::register(registry);
    callback_sets::register(registry);
    walk::register(registry);
    filter::register(registry);
    find::register(registry);
    sort::register(registry);
    sets::register(registry);
    tables::register(registry);
    stack::register(registry);
    build::register(registry);
    range::register(registry);
    combine::register(registry);
    pointer::register(registry);
    keys::register(registry);
    slice::register(registry);
}

/// Expose a key without applying PHP numeric-string key coercion again.
fn key_value(key: &ArrayKey) -> Value {
    match key {
        ArrayKey::Int(integer) => Value::Int(*integer),
        ArrayKey::String(string) => Value::String(string.clone()),
    }
}

#[derive(Clone, Copy)]
pub(super) enum Keys {
    Renumber,
    Preserve,
}

impl Keys {
    fn from_preserve(preserve_keys: bool) -> Self {
        if preserve_keys {
            Self::Preserve
        } else {
            Self::Renumber
        }
    }
}

/// List construction assigns fresh consecutive integer keys in iteration order.
fn list_array(
    values: impl IntoIterator<Item = Value>,
) -> Result<phera_extension_api::PhpArray, phera_extension_api::NativeError> {
    let mut array = phera_extension_api::PhpArray::new();
    for value in values {
        array.push(value)?;
    }
    Ok(array)
}
