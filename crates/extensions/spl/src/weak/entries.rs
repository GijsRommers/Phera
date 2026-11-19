//! A WeakMap's entries: weak handles on the keys, in insertion order; the
//! values are attached to the key objects themselves.
//!
//! ```php
//! $map = new WeakMap;
//! $key = new stdClass;
//! $map[$key] = new Value;   // attached to $key, tagged with this map
//! unset($key);              // the value is freed with the key, at once
//! ```
//!
//! PHP drops an entry, and its value, at the moment the key is freed
//! (after the key's destructor; never, if the destructor keeps `$this`).
//! Rust frees the key in `Drop`, which cannot reach the map, so the map
//! does not own its values: each key object does (see
//! `phera_value::Object::attach`), under this map's serial number. When
//! the map itself goes first, its `Drop` takes its values back from the
//! keys that are still alive.

use std::rc::Rc;

use indexmap::IndexMap;
use phera_extension_api::value::WeakObject;
use phera_extension_api::{Object, Value};

/// The entries of one WeakMap.
#[derive(Default)]
pub(super) struct Entries {
    keys: IndexMap<u32, WeakObject>,
    /// The serial number of the WeakMap: the tag of its values on the keys.
    owner: u64,
}

impl Entries {
    /// Whether `key` is a live key of this map.
    pub(super) fn contains(&self, key: &Rc<Object>) -> bool {
        self.keys
            .get(&key.id)
            .is_some_and(|weak| weak.refers_to(key) && weak.is_alive())
    }

    /// The value stored for `key`.
    pub(super) fn get(&self, key: &Rc<Object>) -> Option<Value> {
        self.contains(key)
            .then(|| key.attachment(self.owner))
            .flatten()
    }

    /// Changes the value stored for `key` in place.
    pub(super) fn update<R>(
        &self,
        key: &Rc<Object>,
        change: impl FnOnce(&mut Value) -> R,
    ) -> Option<R> {
        self.contains(key)
            .then(|| key.update_attachment(self.owner, change))
            .flatten()
    }

    /// Stores `value` for `key` in the map whose serial number is `owner`;
    /// returns the value it replaces, for the caller to drop.
    pub(super) fn set(&mut self, owner: u64, key: &Rc<Object>, value: Value) -> Option<Value> {
        self.owner = owner;
        if self
            .keys
            .get(&key.id)
            .is_some_and(|weak| !weak.refers_to(key))
        {
            // A key that is gone left its id to this object.
            self.keys.shift_remove(&key.id);
        }
        self.keys.insert(key.id, WeakObject::new(key));
        key.attach(owner, value)
    }

    /// Removes `key`; returns its value, for the caller to drop.
    pub(super) fn unset(&mut self, key: &Rc<Object>) -> Option<Value> {
        if !self
            .keys
            .get(&key.id)
            .is_some_and(|weak| weak.refers_to(key))
        {
            return None;
        }
        self.keys.shift_remove(&key.id);
        key.detach(self.owner)
    }

    /// The live entries, in order.
    pub(super) fn live(&self) -> impl Iterator<Item = (Rc<Object>, Value)> + '_ {
        self.keys.values().filter_map(|weak| {
            let key = weak.upgrade()?;
            let value = key.attachment(self.owner)?;
            Some((key, value))
        })
    }

    /// The entry at `index` of the insertion order, if its key is alive.
    pub(super) fn at(&self, index: usize) -> Option<(Rc<Object>, Value)> {
        let (_, weak) = self.keys.get_index(index)?;
        let key = weak.upgrade()?;
        let value = key.attachment(self.owner)?;
        Some((key, value))
    }

    /// How many positions the insertion order has, dead keys included.
    pub(super) fn positions(&self) -> usize {
        self.keys.len()
    }

    /// Forgets keys that are gone, and counts the rest.
    pub(super) fn count(&mut self) -> usize {
        self.keys.retain(|_, weak| weak.is_alive());
        self.keys.len()
    }

    /// The same entries for the WeakMap whose serial number is `owner`.
    pub(super) fn copy_for(&self, owner: u64) -> Entries {
        let mut copy = Entries {
            keys: IndexMap::new(),
            owner,
        };
        for (key, value) in self.live() {
            drop(copy.set(owner, &key, value));
        }
        copy
    }
}

impl Drop for Entries {
    /// The map is gone: its values go too, from the keys still alive.
    fn drop(&mut self) {
        let mut released = Vec::new();
        for weak in self.keys.values() {
            if let Some(key) = weak.upgrade() {
                released.extend(key.detach(self.owner));
            }
        }
        drop(released);
    }
}
