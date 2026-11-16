//! Registration and shared helpers for PHP operations.

use phera_extension_api::{ArrayKey, Registry, Value};

mod build;
mod combine;
mod keys;
mod ordering;
mod pointer;
mod range;
mod sets;
mod slice;
mod sort;
mod sort_order;
mod stack;
mod tables;

pub(crate) fn register(registry: &mut Registry) {
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

fn key_value(key: &ArrayKey) -> Value {
    match key {
        ArrayKey::Int(i) => Value::Int(*i),
        ArrayKey::String(string) => Value::String(string.clone()),
    }
}
