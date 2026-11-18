//! Standard PHP Library containers and iterator protocols.
//!
//! ```php
//! $items = new ArrayObject(['key' => 'value']);
//! foreach ($items as $key => $value) {}
//! ```

use phera_extension_api::{
    ArrayKey, Context, Extension, NativeError, Object, PhpArray, Registry, Value,
};
use std::rc::Rc;

mod arrays;
mod catalog;
mod fixed_array;
mod heap;
mod interfaces;
mod legacy;
mod linked_list;
mod multiple_iterator;
mod native_state;
mod object_storage;
mod recursive;
mod weak;
mod wrappers;

pub struct Spl;

impl Extension for Spl {
    fn name(&self) -> &'static str {
        "spl"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("SPL")
    }

    fn register(&self, registry: &mut Registry) {
        interfaces::register(registry);
        arrays::register(registry);
        object_storage::register(registry);
        weak::register(registry);
        linked_list::register(registry);
        fixed_array::register(registry);
        heap::register(registry);
        wrappers::register(registry);
        multiple_iterator::register(registry);
        recursive::register(registry);
        catalog::register(registry);
    }
}

type This<'a> = Option<&'a Rc<Object>>;

fn receiver(this: This<'_>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("Missing SPL method receiver"))
}

use native_state::{read_state, store, update_state};

fn busy() -> NativeError {
    NativeError::error("Cannot access SPL storage while it is being modified")
}

fn make<T: 'static>(
    context: &mut dyn Context,
    class: &str,
    state: T,
) -> Result<Rc<Object>, NativeError> {
    let Value::Object(object) = context.create_object(class, PhpArray::new())? else {
        return Err(NativeError::error(
            "SPL class factory did not return an object",
        ));
    };
    store(&object, state)?;
    Ok(object)
}

fn list(values: impl IntoIterator<Item = Value>) -> Value {
    Value::from(
        values
            .into_iter()
            .enumerate()
            .map(|(index, value)| (ArrayKey::Int(index as i64), value))
            .collect::<PhpArray>(),
    )
}

fn warn_string_key(context: &mut dyn Context, key: &[u8]) {
    let key = key.split(|byte| *byte == 0).next().unwrap_or_default();
    let mut message = b"Undefined array key \"".to_vec();
    message.extend_from_slice(key);
    message.push(b'"');
    context.warning_bytes(&message);
}
