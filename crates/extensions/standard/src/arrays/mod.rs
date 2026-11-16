//! Registration and shared helpers for PHP operations.

use phera_extension_api::{ArrayKey, Registry, Value};

mod keys;
mod pointer;
mod slice;

pub(crate) fn register(registry: &mut Registry) {
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
