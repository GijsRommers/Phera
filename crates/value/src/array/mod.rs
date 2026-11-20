//! A PHP array: an ordered map from integer or string keys to values.
//!
//! ```php
//! $a = [10, 20];       // a list: keys 0 and 1 are positions, not stored
//! $a['name'] = 'x';    // a string key makes it a map of key-value entries
//! unset($a[0]);        // leaves a hole; later entries keep their positions
//! $a[] = 30;           // key 2: one more than the largest integer key so far
//! ```
//!
//! The two storage shapes, both in insertion order:
//!
//! ```text
//! [10, 20, 30]             List: [Some(10), Some(20), Some(30)]
//! ['name' => 10, 7 => 20]  Map:  [('name', Some(10)), (7, Some(20))]
//!                                up to 8 slots: compare keys one by one
//!                                above 8: an index from hash(key) to position
//! ```
//!
//! A string key, or an integer key that is not the next position, turns a
//! list into a map. Unset leaves a hole, because a running foreach remembers
//! positions, so removal must not move later values (trailing holes
//! included; see php-src test gh13178_5). Holes are dropped later, when an
//! append or pop finds more holes than values, and by shift, which also
//! numbers the integer keys again and can turn a map back into a list.
//! Copy-on-write is `Rc::make_mut` on the `Rc<PhpArray>` inside a `Value`.

mod drop;
mod iteration;
mod key;
mod lookup;
mod mutation;
mod pointer;
mod separation;
mod storage;
#[cfg(test)]
mod tests;

use crate::value::Value;
pub use key::ArrayKey;
use std::fmt;
use storage::Storage;

#[derive(Default)]
pub struct PhpArray {
    storage: Storage,
    /// The key `$a[]` uses next: one more than the largest integer key ever
    /// inserted, `None` before the first. It stops at `i64::MAX`.
    next_free: Option<i64>,
    /// Number of holes in `storage`.
    dead: u32,
    /// The internal pointer of `current()` and `next()`: a slot position;
    /// see `pointer.rs`.
    pos: u32,
}

impl PhpArray {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            storage: Storage::List(Vec::with_capacity(cap)),
            ..Self::default()
        }
    }

    pub fn len(&self) -> usize {
        self.storage.len() - self.dead as usize
    }

    /// The bytes this array holds on the heap for its slots and its index
    /// (by capacity), not counting what its values point to. For the
    /// footprint of a fork (see `crate::fork::Fork::footprint`).
    pub fn heap_bytes(&self) -> usize {
        use std::mem::size_of;
        match &self.storage {
            storage::Storage::List(slots) => slots.capacity() * size_of::<Option<Value>>(),
            storage::Storage::Map(map) => {
                map.entries.capacity() * size_of::<storage::Entry>()
                    + map.index.as_ref().map_or(0, |index| {
                        size_of::<storage::Index>()
                            + index.positions.capacity() * (size_of::<u32>() + 1)
                    })
            }
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The key `$a[]` would use next; see the field `next_free`.
    pub fn next_free_index(&self) -> Option<i64> {
        self.next_free
    }

    /// `array_is_list`: keys 0, 1, 2, ... in order. A list with holes is not
    /// one, and a map can be one: `$a = ['x' => 1, 0 => 2]; unset($a['x']);`
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
    /// Reserves once when the iterator knows its length.
    fn from_iter<T: IntoIterator<Item = (ArrayKey, Value)>>(iter: T) -> Self {
        let iter = iter.into_iter();
        let mut array = Self::with_capacity(iter.size_hint().0);
        for (key, value) in iter {
            array.insert(key, value);
        }
        array
    }
}
