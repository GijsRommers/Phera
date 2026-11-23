//! Reading by key: `$a[5]`, `$a['name']`, `isset($a['name'])`.

use super::key::numeric_string_key;
use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::string::PhpString;
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

    /// Reads an integer-keyed element without constructing an [`ArrayKey`].
    #[inline]
    pub fn get_int(&self, key: i64) -> Option<&Value> {
        match &self.storage {
            Storage::List(values) => values.get(usize::try_from(key).ok()?)?.as_ref(),
            Storage::Map(map) => map.entries[map.find_int(key)?].value.as_ref(),
        }
    }

    /// Reads a string-keyed element (applying PHP numeric-string key
    /// normalisation) without cloning the [`PhpString`] handle.
    #[inline]
    pub fn get_str(&self, key: &PhpString) -> Option<&Value> {
        if let Some(index) = numeric_string_key(key.as_bytes()) {
            return self.get_int(index);
        }
        match &self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => map.entries[map.find_str(key)?].value.as_ref(),
        }
    }

    /// Reads `$array[$key]` for integer or string keys. The outer option
    /// distinguishes an unsupported key from an absent element.
    #[inline]
    pub fn get_value_key(&self, key: &Value) -> Option<Option<&Value>> {
        match key {
            Value::Int(key) => Some(self.get_int(*key)),
            Value::String(key) => Some(self.get_str(key)),
            _ => None,
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

    /// Mutably borrows an integer-keyed element without constructing an [`ArrayKey`].
    #[inline]
    pub fn get_mut_int(&mut self, key: i64) -> Option<&mut Value> {
        match &mut self.storage {
            Storage::List(values) => values.get_mut(usize::try_from(key).ok()?)?.as_mut(),
            Storage::Map(map) => {
                let position = map.find_int(key)?;
                map.entries[position].value.as_mut()
            }
        }
    }

    /// Mutably borrows a string-keyed element (applying PHP numeric-string key
    /// normalisation) without cloning the [`PhpString`] handle.
    #[inline]
    pub fn get_mut_str(&mut self, key: &PhpString) -> Option<&mut Value> {
        if let Some(index) = numeric_string_key(key.as_bytes()) {
            return self.get_mut_int(index);
        }
        match &mut self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => {
                let position = map.find_str(key)?;
                map.entries[position].value.as_mut()
            }
        }
    }

    #[inline]
    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.find_pos(key).is_some()
    }

    #[inline]
    pub fn contains_int(&self, key: i64) -> bool {
        self.get_int(key).is_some()
    }

    #[inline]
    pub fn contains_str(&self, key: &PhpString) -> bool {
        self.get_str(key).is_some()
    }
}
