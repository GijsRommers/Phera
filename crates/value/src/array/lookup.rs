//! Resolve keys without duplicating them in a lookup table.

use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::value::Value;
use std::hash::BuildHasher;

impl PhpArray {
    /// Lists compute the position; maps compare against their single stored key.
    pub(super) fn find_pos(&self, key: &ArrayKey) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => match key {
                ArrayKey::Int(index) => usize::try_from(*index)
                    .ok()
                    .filter(|&position| values.get(position).is_some_and(Option::is_some)),
                ArrayKey::String(_) => None,
            },
            Storage::Map(map) => match &map.positions {
                None => map
                    .entries
                    .iter()
                    .position(|entry| entry.value.is_some() && &entry.key == key),
                Some(table) => table
                    .find(map.hasher.hash_one(key), |position| {
                        &map.entries[*position as usize].key == key
                    })
                    .map(|position| *position as usize),
            },
        }
    }

    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        self.storage.value(self.find_pos(key)?)
    }

    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        let position = self.find_pos(key)?;
        self.storage.slot_mut(position)?.as_mut()
    }

    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.find_pos(key).is_some()
    }
}
