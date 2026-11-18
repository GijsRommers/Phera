//! Reading by key: `$a[5]`, `$a['name']`, `isset($a['name'])`.

use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::value::Value;

/// The list position a key names: only a non-negative integer can name one.
#[inline]
pub(super) fn list_position(key: &ArrayKey) -> Option<usize> {
    match key {
        ArrayKey::Int(index) => usize::try_from(*index).ok(),
        ArrayKey::String(_) => None,
    }
}

impl PhpArray {
    /// Position of the live slot with this key.
    #[inline]
    pub(super) fn find_pos(&self, key: &ArrayKey) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => list_position(key)
                .filter(|&position| values.get(position).is_some_and(Option::is_some)),
            Storage::Map(map) => map.find(key),
        }
    }

    #[inline]
    pub fn get(&self, key: &ArrayKey) -> Option<&Value> {
        match &self.storage {
            Storage::List(values) => values.get(list_position(key)?)?.as_ref(),
            Storage::Map(map) => map.entries[map.find(key)?].value.as_ref(),
        }
    }

    #[inline]
    pub fn get_mut(&mut self, key: &ArrayKey) -> Option<&mut Value> {
        match &mut self.storage {
            Storage::List(values) => values.get_mut(list_position(key)?)?.as_mut(),
            Storage::Map(map) => {
                let position = map.find(key)?;
                map.entries[position].value.as_mut()
            }
        }
    }

    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.find_pos(key).is_some()
    }
}
