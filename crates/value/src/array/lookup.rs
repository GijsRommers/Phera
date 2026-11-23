//! Reading by key: `$a[5]`, `$a['name']`, `isset($a['name'])`.

use super::key::numeric_string_key;
use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::intern::InternedStr;
use crate::string::PhpString;
use crate::value::Value;

/// The list position a key names: only a non-negative integer can name one.
#[inline]
pub(super) fn list_position(key: &ArrayKey) -> Option<usize> {
    match key {
        ArrayKey::Int(index) => usize::try_from(*index).ok(),
        ArrayKey::Interned(_) | ArrayKey::String(_) => None,
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
    pub fn get_int(&self, key: i64) -> Option<&Value> {
        match &self.storage {
            Storage::List(values) => values.get(usize::try_from(key).ok()?)?.as_ref(),
            Storage::Map(map) => map.entries[map.find_int(key)?].value.as_ref(),
        }
    }

    /// Borrowed lookup by an interned string: uses the pre-computed numeric
    /// classification and hash with zero allocation and pointer-equality hits.
    #[inline]
    pub fn get_interned(&self, key: &'static InternedStr) -> Option<&Value> {
        if let Some(int_key) = key.numeric_key() {
            return self.get_int(int_key);
        }
        match &self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => map.entries[map.find_interned(key)?].value.as_ref(),
        }
    }

    /// Borrowed lookup by a `PhpString` without cloning an `ArrayKey`.
    #[inline]
    pub fn get_str(&self, key: &PhpString) -> Option<&Value> {
        self.get_str_bytes(key.as_bytes())
    }

    /// Borrowed lookup by raw string bytes with PHP numeric-key normalization.
    #[inline]
    pub fn get_str_bytes(&self, bytes: &[u8]) -> Option<&Value> {
        if let Some(int_key) = numeric_string_key(bytes) {
            return self.get_int(int_key);
        }
        match &self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => map.entries[map.find_str_bytes(bytes)?].value.as_ref(),
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

    #[inline]
    pub fn get_mut_interned(&mut self, key: &'static InternedStr) -> Option<&mut Value> {
        if let Some(int_key) = key.numeric_key() {
            return self.get_mut_int(int_key);
        }
        match &mut self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => {
                let position = map.find_interned(key)?;
                map.entries[position].value.as_mut()
            }
        }
    }

    #[inline]
    pub fn get_mut_str(&mut self, key: &PhpString) -> Option<&mut Value> {
        if let Some(int_key) = numeric_string_key(key.as_bytes()) {
            return self.get_mut_int(int_key);
        }
        match &mut self.storage {
            Storage::List(_) => None,
            Storage::Map(map) => {
                let position = map.find_str_bytes(key.as_bytes())?;
                map.entries[position].value.as_mut()
            }
        }
    }

    pub fn contains_key(&self, key: &ArrayKey) -> bool {
        self.find_pos(key).is_some()
    }

    #[inline]
    pub fn contains_int(&self, key: i64) -> bool {
        self.get_int(key).is_some()
    }

    #[inline]
    pub fn contains_interned(&self, key: &'static InternedStr) -> bool {
        self.get_interned(key).is_some()
    }

    #[inline]
    pub fn contains_str(&self, key: &PhpString) -> bool {
        self.get_str(key).is_some()
    }
}
