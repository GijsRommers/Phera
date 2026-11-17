//! PHP array registration and shared key-retention and list-construction helpers.

use phera_extension_api::{ArrayKey, PhpArray, Registry, Value};

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
mod stack;
mod tables;
mod walk;
pub(crate) mod zend_sort;

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

/// A PHP list, `[0 => $a, 1 => $b, …]`, of these values.
pub(crate) fn list(values: impl IntoIterator<Item = Value>) -> PhpArray {
    let mut array = PhpArray::new();
    for value in values {
        // Only an array whose next key would pass PHP_INT_MAX refuses a push.
        array
            .push(value)
            .expect("a new array has a free key for every value");
    }
    array
}
