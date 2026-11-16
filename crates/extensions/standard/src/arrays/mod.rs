mod keys;
mod slice;
use phera_extension_api::*;
use std::rc::Rc;
pub(crate) fn register(r: &mut Registry) {
    keys::register(r);
    slice::register(r);
}
fn key_value(k: &ArrayKey) -> Value {
    match k {
        ArrayKey::Int(i) => Value::Int(*i),
        ArrayKey::String(s) => Value::String(s.clone()),
    }
}
