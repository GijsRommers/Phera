//! The two storage shapes behind `PhpArray`, and the index of a large map.
//!
//! A slot that holds `None` is a hole left by `unset`. Holes keep the
//! positions of later slots fixed, because a running `foreach` remembers a
//! position, not a key.

use foldhash::fast::RandomState;
use hashbrown::HashTable;
use std::hash::BuildHasher;

use super::ArrayKey;
use crate::value::Value;

/// A map with at most this many slots finds a key by comparing it with each
/// stored key in turn, and has no index. Measured when the two shapes were
/// written: building and reading a map of 8 string keys takes about 60% of
/// the cycles by scanning that it takes with an index, 4 keys about 50%.
pub(super) const SCAN_LIMIT: usize = 8;

/// One slot of a map: the key is stored once, here.
#[derive(Clone)]
pub(super) struct Entry {
    pub key: ArrayKey,
    pub value: Option<Value>,
}

#[derive(Clone)]
pub(super) enum Storage {
    /// Keys 0, 1, 2, ... are the positions and are not stored.
    List(Vec<Option<Value>>),
    Map(OrderedMap),
}

impl Default for Storage {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

/// Entries in insertion order. The index is boxed so that a small map,
/// which has none, pays one pointer for it.
#[derive(Clone)]
pub(super) struct OrderedMap {
    pub entries: Vec<Entry>,
    pub index: Option<Box<Index>>,
}

/// A hash table of entry positions. It stores no keys: hashing and
/// comparing go through the key in the entry, which is the only copy.
/// Each array has its own random seed, so crafted keys cannot make one
/// array slow (hash flooding).
#[derive(Clone)]
pub(super) struct Index {
    pub positions: HashTable<u32>,
    pub hasher: RandomState,
}

impl Storage {
    /// Number of slots, holes included.
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::List(values) => values.len(),
            Self::Map(map) => map.entries.len(),
        }
    }

    /// Drops every slot from `position` on. The caller has taken their
    /// values out of the index already (only holes and the removed entry
    /// are past `position`).
    pub fn truncate(&mut self, position: usize) {
        match self {
            Self::List(values) => values.truncate(position),
            Self::Map(map) => map.entries.truncate(position),
        }
    }

    /// The value at `position`, or `None` for a hole or past the end.
    #[inline]
    pub fn value(&self, position: usize) -> Option<&Value> {
        match self {
            Self::List(values) => values.get(position)?.as_ref(),
            Self::Map(map) => map.entries.get(position)?.value.as_ref(),
        }
    }

    /// The slot at `position`, so it can be filled or emptied.
    #[inline]
    pub fn slot_mut(&mut self, position: usize) -> Option<&mut Option<Value>> {
        match self {
            Self::List(values) => values.get_mut(position),
            Self::Map(map) => map.entries.get_mut(position).map(|entry| &mut entry.value),
        }
    }

    /// The storage as a map, converting a list first. Holes stay holes, so
    /// every position is unchanged.
    pub fn as_map(&mut self) -> &mut OrderedMap {
        if let Self::List(values) = self {
            let entries = std::mem::take(values)
                .into_iter()
                .enumerate()
                .map(|(position, value)| Entry {
                    key: ArrayKey::Int(position as i64),
                    value,
                })
                .collect();
            *self = Self::Map(OrderedMap::new(entries));
        }
        match self {
            Self::Map(map) => map,
            Self::List(_) => unreachable!("a list was converted to a map above"),
        }
    }

    /// Storage for entries that have no holes: a list when their keys are
    /// 0, 1, 2, ... in order, else a map. Compaction and shift use this to
    /// turn a map back into a list.
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        let sequential = entries
            .iter()
            .enumerate()
            .all(|(position, entry)| entry.key == ArrayKey::Int(position as i64));
        if sequential {
            Self::List(entries.into_iter().map(|entry| entry.value).collect())
        } else {
            Self::Map(OrderedMap::new(entries))
        }
    }
}

impl OrderedMap {
    pub fn new(entries: Vec<Entry>) -> Self {
        let index = Index::build(&entries);
        Self { entries, index }
    }

    /// Position of the live entry with this key.
    #[inline]
    pub fn find(&self, key: &ArrayKey) -> Option<usize> {
        match &self.index {
            None => self
                .entries
                .iter()
                .position(|entry| entry.value.is_some() && entry.key == *key),
            Some(index) => index.find(&self.entries, key),
        }
    }

    /// Appends an entry whose key is not in the map yet. The entry is pushed
    /// before it is indexed, so that a growing table can rehash through it.
    pub fn append(&mut self, key: ArrayKey, value: Value) {
        self.entries.push(Entry {
            key,
            value: Some(value),
        });
        match &mut self.index {
            Some(index) => index.insert(&self.entries, self.entries.len() - 1),
            None if self.entries.len() > SCAN_LIMIT => self.index = Index::build(&self.entries),
            None => {}
        }
    }

    /// Unsets the live entry with this key.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let position = match &mut self.index {
            None => self.find(key)?,
            Some(index) => index.remove(&self.entries, key)?,
        };
        let (_, value) = self.make_hole(position);
        value
    }

    /// Unsets the entry at a live `position` and returns its key and value.
    pub fn remove_at(&mut self, position: usize) -> (ArrayKey, Option<Value>) {
        if let Some(index) = &mut self.index {
            index.remove_position(&self.entries, position);
        }
        self.make_hole(position)
    }

    /// Replaces the key of a hole with `Int(0)`, so that a string key is
    /// freed now rather than when the array is compacted.
    fn make_hole(&mut self, position: usize) -> (ArrayKey, Option<Value>) {
        let entry = &mut self.entries[position];
        (
            std::mem::replace(&mut entry.key, ArrayKey::Int(0)),
            entry.value.take(),
        )
    }
}

impl Index {
    /// The index for `entries`, or `None` when they are few enough to scan.
    fn build(entries: &[Entry]) -> Option<Box<Self>> {
        if entries.len() <= SCAN_LIMIT {
            return None;
        }
        let mut index = Self {
            positions: HashTable::with_capacity(entries.len()),
            hasher: RandomState::default(),
        };
        for (position, entry) in entries.iter().enumerate() {
            if entry.value.is_some() {
                index.insert(entries, position);
            }
        }
        Some(Box::new(index))
    }

    fn find(&self, entries: &[Entry], key: &ArrayKey) -> Option<usize> {
        let hash = self.hasher.hash_one(key);
        self.positions
            .find(hash, |&position| entries[position as usize].key == *key)
            .map(|&position| position as usize)
    }

    fn insert(&mut self, entries: &[Entry], position: usize) {
        let hasher = &self.hasher;
        let position = u32::try_from(position).expect("array position fits u32");
        self.positions.insert_unique(
            hasher.hash_one(&entries[position as usize].key),
            position,
            |&other| hasher.hash_one(&entries[other as usize].key),
        );
    }

    /// Drops the key from the index and returns where its entry is.
    fn remove(&mut self, entries: &[Entry], key: &ArrayKey) -> Option<usize> {
        let hash = self.hasher.hash_one(key);
        let (position, _) = self
            .positions
            .find_entry(hash, |&position| entries[position as usize].key == *key)
            .ok()?
            .remove();
        Some(position as usize)
    }

    /// Drops the live entry at `position` from the index. It compares
    /// positions, not keys, which is cheaper.
    fn remove_position(&mut self, entries: &[Entry], position: usize) {
        let hash = self.hasher.hash_one(&entries[position].key);
        self.positions
            .find_entry(hash, |&other| other as usize == position)
            .expect("every live entry is in the index")
            .remove();
    }
}
