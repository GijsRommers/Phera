//! PHP arrays have two storage shapes, both in insertion order:
//!
//! ```text
//! [10, 20, 30]           List: [Some(10), Some(20), Some(30)]
//!                             keys 0, 1, 2 are computed, never stored
//! ['name' => 10, 7 => 20] Map:  [('name', Some(10)), (7, Some(20))]
//!                             index: hash(key) -> entry position (u32)
//! ```
//!
//! A string or out-of-order integer key turns a list into a map. Unset leaves
//! a hole: running foreach loops remember positions, so removal must not move
//! later values (including trailing holes; see php-src gh13178_5). Shift
//! renumbers integer keys and can turn a map back into a list. Copy-on-write
//! clones the active shape through Rc::make_mut.

mod drop;
mod iteration;
mod key;
#[cfg(test)]
mod key_tests;
mod lookup;
#[cfg(test)]
mod model_tests;
mod mutation;
mod pointer;
mod storage;
#[cfg(test)]
mod storage_tests;
#[cfg(test)]
mod tests;

use crate::value::Value;
pub use key::ArrayKey;
use std::fmt;
use storage::Storage;

/// Internal-pointer sentinel. Stored positions fit the same u32 as the index.
const INVALID: u32 = u32::MAX;

/// An ordered PHP array. Only maps allocate a key index; lists store values alone.
#[derive(Clone, Default)]
pub struct PhpArray {
    storage: Storage,
    /// None means no integer key has been inserted. MAX prevents append.
    next_free: Option<i64>,
    dead: u32,
    pos: u32,
}

impl PhpArray {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            storage: Storage::List(Vec::with_capacity(cap)),
            pos: INVALID,
            ..Self::default()
        }
    }

    pub fn len(&self) -> usize {
        self.storage.len() - self.dead as usize
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Exposes the historical next-free state, including MAX after overflow.
    pub fn next_free_index(&self) -> Option<i64> {
        self.next_free
    }

    /// Holes and out-of-order keys disqualify a PHP list even in packed storage.
    pub fn is_list(&self) -> bool {
        if matches!(self.storage, Storage::List(_)) && self.dead == 0 {
            return true;
        }
        self.keys()
            .enumerate()
            .all(|(index, key)| key == ArrayKey::Int(index as i64))
    }
}

impl fmt::Debug for PhpArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl FromIterator<(ArrayKey, Value)> for PhpArray {
    /// Reserve once when the producer knows its length, preserving insertion order.
    fn from_iter<T: IntoIterator<Item = (ArrayKey, Value)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut array = Self::with_capacity(iter.size_hint().0);
        for (key, value) in iter {
            array.insert(key, value);
        }
        array
    }
}
