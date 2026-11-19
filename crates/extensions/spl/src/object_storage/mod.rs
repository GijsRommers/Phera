//! SplObjectStorage maps object identity to associated PHP values.
//!
//! ```php
//! $storage = new SplObjectStorage;
//! $storage[$object] = 'info';
//! ```

mod dimensions;
mod iteration;
mod registration;
mod views;

pub(crate) use registration::register;

use std::rc::Rc;

use indexmap::IndexMap;
use phera_extension_api::{Context, NativeError, Object, PhpString, Value};

const CLASS: &str = "SplObjectStorage";

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Key {
    Handle(u32),
    Custom(PhpString),
}

#[derive(Clone)]
struct Entry {
    object: Value,
    info: Value,
}

#[derive(Clone, Default)]
struct Storage {
    entries: IndexMap<Key, Entry>,
    slot: usize,
    index: i64,
}

fn read<T>(object: &Object, f: impl FnOnce(&Storage) -> T) -> Result<T, NativeError> {
    crate::read_state(object, f)
}

fn update<T>(object: &Object, f: impl FnOnce(&mut Storage) -> T) -> Result<T, NativeError> {
    crate::update_state(object, f)
}

async fn hash(
    context: &mut dyn Context,
    storage: &Rc<Object>,
    object: &Rc<Object>,
) -> Result<Key, NativeError> {
    if !context.is_user_method(storage, "getHash") {
        return Ok(Key::Handle(object.id));
    }
    let value = context
        .call_method(
            storage.clone(),
            "getHash",
            vec![Value::Object(object.clone())],
        )
        .await?;
    if let Value::String(string) = &*value.deref() {
        return Ok(Key::Custom(string.clone()));
    }
    Err(NativeError::type_error(format!(
        "{}::getHash() must return a string",
        storage.class.name()
    )))
}

fn attach(storage: &Object, key: Key, object: Rc<Object>, info: Value) -> Result<(), NativeError> {
    update(storage, |storage| match storage.entries.entry(key) {
        indexmap::map::Entry::Occupied(mut entry) => entry.get_mut().info = info,
        indexmap::map::Entry::Vacant(entry) => {
            entry.insert(Entry {
                object: Value::Object(object),
                info,
            });
        }
    })
}

fn detach(storage: &Object, key: &Key, rewind: bool) -> Result<(), NativeError> {
    update(storage, |storage| {
        let removed = storage.entries.shift_remove_full(key);
        if rewind {
            storage.slot = 0;
            storage.index = 0;
        } else if removed.is_some_and(|(slot, _, _)| slot < storage.slot) {
            storage.slot -= 1;
        }
    })
}

/// Keys are object ids, which a fork keeps.
impl crate::native_state::ForkValues for Storage {
    fn fork_values(
        &mut self,
        fork: &mut phera_extension_api::value::Fork<'_>,
    ) -> Result<(), String> {
        for entry in self.entries.values_mut() {
            crate::native_state::fork_value(&mut entry.object, fork);
            crate::native_state::fork_value(&mut entry.info, fork);
        }
        Ok(())
    }
}

pub(crate) const FORK: crate::native_state::StoredFork = crate::native_state::fork_as::<Storage>;
