//! The two storage shapes behind `PhpArray`, and the index of a large map.
//!
//! A slot that holds `None` is a hole left by `unset`. Holes keep the
//! positions of later slots fixed, because a running `foreach` remembers a
//! position, not a key.

use foldhash::fast::RandomState;
use hashbrown::HashTable;
use hashbrown::hash_table::Entry as HashEntry;
use std::hash::BuildHasher;
use std::sync::OnceLock;

use super::ArrayKey;
use crate::intern::InternedStr;
use crate::value::Value;

/// A map with at most this many slots finds a key by comparing it with each
/// stored key in turn, and has no index. Measured when the two shapes were
/// written: building and reading a map of 8 string keys takes about 60% of
/// the cycles by scanning that it takes with an index, 4 keys about 50%.
pub(super) const SCAN_LIMIT: usize = 8;

/// Process-wide randomized hash seed shared across all arrays and interned
/// strings, matching Zend's per-process seed so pre-computed `InternedStr`
/// hashes work directly in every `PhpArray` without per-array rehashing.
#[inline(always)]
fn array_hasher() -> &'static RandomState {
    static HASHER: OnceLock<RandomState> = OnceLock::new();
    HASHER.get_or_init(RandomState::default)
}

#[inline]
pub(crate) fn hash_str_bytes(bytes: &[u8]) -> u64 {
    array_hasher().hash_one(bytes)
}

#[inline]
pub(super) fn hash_int_key(key: i64) -> u64 {
    array_hasher().hash_one(key)
}

#[inline]
fn hash_array_key(key: &ArrayKey) -> u64 {
    match key {
        ArrayKey::Int(i) => hash_int_key(*i),
        ArrayKey::Interned(s) => s.array_hash(),
        ArrayKey::String(s) => hash_str_bytes(s.as_bytes()),
    }
}

/// One slot of a map: the key is stored once, here.
#[derive(Clone)]
pub(super) struct Entry {
    pub key: ArrayKey,
    pub value: Option<Value>,
}

#[derive(Clone)]
pub(super) enum Storage {
    /// Consecutive integer keys `list_start, list_start + 1, ...` stored without keys.
    /// Keeping `List` at 24 bytes lets the enum tag sit in `OrderedMap`'s niche so
    /// `size_of::<Storage>() == 32` and `size_of::<PhpArray>() == 56`.
    List(Vec<Option<Value>>),
    Map(OrderedMap),
}

/// Maps an integer key to its slot position in a `Storage::List` starting at `start`.
#[inline(always)]
pub(super) fn list_index(start: i32, key: i64) -> Option<usize> {
    let base = i64::from(start);
    if key < base {
        return None;
    }
    usize::try_from(key.abs_diff(base)).ok()
}

/// Maps a slot position in a `Storage::List` starting at `start` to its integer key.
#[inline(always)]
pub(super) fn list_key(start: i32, position: usize) -> i64 {
    i64::from(start) + position as i64
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

/// A hash table of entry positions. It stores no keys: comparing goes
/// through the key in the entry, which is the only copy. Each slot keeps 32
/// bits of its key's hash beside the position, as PHP's buckets keep the
/// hash: growing the table never hashes a key again, and a probe that meets
/// another key's slot is turned away without reading that entry.
#[derive(Clone)]
pub(super) struct Index {
    pub positions: HashTable<Slot>,
}

/// One slot of an [`Index`]: where the entry is, and its key's hash.
#[derive(Clone, Copy, Debug)]
pub(super) struct Slot {
    position: u32,
    hash: u32,
}

impl Slot {
    #[inline(always)]
    fn new(position: usize, hash: u32) -> Self {
        let position = u32::try_from(position).expect("array position fits u32");
        Self { position, hash }
    }

    #[inline(always)]
    fn position(self) -> usize {
        self.position as usize
    }

    /// The hash the table files this slot under. Only 32 bits are kept, so
    /// they go to both halves: the table takes its bucket from the low bits
    /// and its control byte from the top ones.
    #[inline(always)]
    fn table_hash(self) -> u64 {
        spread(self.hash)
    }
}

/// The 32 bits of a key's hash that an [`Index`] keeps.
#[inline(always)]
fn short_hash(hash: u64) -> u32 {
    (hash >> 32) as u32
}

#[inline(always)]
fn spread(short: u32) -> u64 {
    let short = u64::from(short);
    short | (short << 32)
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
    pub fn as_map(&mut self, list_start: i32) -> &mut OrderedMap {
        if let Self::List(values) = self {
            let mut entries = Vec::with_capacity(values.capacity());
            entries.extend(std::mem::take(values).into_iter().enumerate().map(
                |(position, value)| Entry {
                    key: ArrayKey::Int(list_key(list_start, position)),
                    value,
                },
            ));
            *self = Self::Map(OrderedMap::new(entries));
        }
        match self {
            Self::Map(map) => map,
            Self::List(_) => unreachable!("a list was converted to a map above"),
        }
    }

    /// Storage for entries that have no holes: a list when their keys are
    /// consecutive integers starting from an `i32` offset, else a map.
    /// Compaction and shift use this to turn a map back into a list.
    pub fn from_entries(entries: Vec<Entry>) -> (i32, Self) {
        let Some(first) = entries.first() else {
            return (0, Self::List(Vec::new()));
        };
        if let ArrayKey::Int(base) = first.key
            && let Ok(start) = i32::try_from(base)
            && entries
                .iter()
                .enumerate()
                .all(|(position, entry)| entry.key == ArrayKey::Int(list_key(start, position)))
        {
            (
                start,
                Self::List(entries.into_iter().map(|entry| entry.value).collect()),
            )
        } else {
            (0, Self::Map(OrderedMap::new(entries)))
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

    /// Position of the live entry with integer key `key`.
    #[inline]
    pub fn find_int(&self, key: i64) -> Option<usize> {
        match &self.index {
            None => self.entries.iter().position(|entry| {
                entry.value.is_some() && matches!(entry.key, ArrayKey::Int(k) if k == key)
            }),
            Some(index) => index.find_int(&self.entries, key),
        }
    }

    /// Position of the live entry with non-numeric interned string key `key`.
    #[inline]
    pub fn find_interned(&self, key: &'static InternedStr) -> Option<usize> {
        match &self.index {
            None => self.entries.iter().position(|entry| {
                entry.value.is_some()
                    && match &entry.key {
                        ArrayKey::Interned(k) => std::ptr::eq(*k, key),
                        ArrayKey::String(k) => k.as_bytes() == key.as_bytes(),
                        ArrayKey::Int(_) => false,
                    }
            }),
            Some(index) => index.find_interned(&self.entries, key),
        }
    }

    /// Position of the live entry with non-numeric string key bytes `bytes`.
    #[inline]
    pub fn find_str_bytes(&self, bytes: &[u8]) -> Option<usize> {
        match &self.index {
            None => self.entries.iter().position(|entry| {
                entry.value.is_some()
                    && match &entry.key {
                        ArrayKey::Interned(k) => k.as_bytes() == bytes,
                        ArrayKey::String(k) => k.as_bytes() == bytes,
                        ArrayKey::Int(_) => false,
                    }
            }),
            Some(index) => index.find_str_bytes(&self.entries, bytes),
        }
    }

    /// Single-probe insert or update that refuses to overwrite a PHP
    /// reference, returning `Err(value)` untouched when the slot is a
    /// reference. The key is hashed once, whether it is found or added.
    pub fn insert_if_not_ref(
        &mut self,
        key: ArrayKey,
        value: Value,
    ) -> Result<Option<Value>, Value> {
        match &mut self.index {
            None => {
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.value.is_some() && entry.key == key)
                {
                    if entry.value.as_ref().is_some_and(Value::is_reference) {
                        return Err(value);
                    }
                    return Ok(entry.value.replace(value));
                }
                self.entries.push(Entry {
                    key,
                    value: Some(value),
                });
                if self.entries.len() > SCAN_LIMIT {
                    self.index = Index::build(&self.entries);
                }
                Ok(None)
            }
            Some(table) => {
                let short = short_hash(hash_array_key(&key));
                let entries = &mut self.entries;
                match table.positions.entry(
                    spread(short),
                    |slot| slot.hash == short && entries[slot.position()].key == key,
                    |slot| slot.table_hash(),
                ) {
                    HashEntry::Occupied(occupied) => {
                        let position = occupied.get().position();
                        if entries[position]
                            .value
                            .as_ref()
                            .is_some_and(Value::is_reference)
                        {
                            return Err(value);
                        }
                        Ok(entries[position].value.replace(value))
                    }
                    HashEntry::Vacant(vacant) => {
                        vacant.insert(Slot::new(entries.len(), short));
                        entries.push(Entry {
                            key,
                            value: Some(value),
                        });
                        Ok(None)
                    }
                }
            }
        }
    }

    /// Single-probe insert or update: replaces an existing live entry's value
    /// and returns it, or appends a new entry and returns `None`.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        match &mut self.index {
            None => {
                if let Some(entry) = self
                    .entries
                    .iter_mut()
                    .find(|entry| entry.value.is_some() && entry.key == key)
                {
                    return entry.value.replace(value);
                }
                self.entries.push(Entry {
                    key,
                    value: Some(value),
                });
                if self.entries.len() > SCAN_LIMIT {
                    self.index = Index::build(&self.entries);
                }
                None
            }
            Some(index) => {
                let short = short_hash(hash_array_key(&key));
                let entries = &mut self.entries;
                match index.positions.entry(
                    spread(short),
                    |slot| slot.hash == short && entries[slot.position()].key == key,
                    |slot| slot.table_hash(),
                ) {
                    HashEntry::Occupied(occupied) => {
                        let position = occupied.get().position();
                        entries[position].value.replace(value)
                    }
                    HashEntry::Vacant(vacant) => {
                        vacant.insert(Slot::new(entries.len(), short));
                        entries.push(Entry {
                            key,
                            value: Some(value),
                        });
                        None
                    }
                }
            }
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

    /// Unsets the live entry with integer key `key`.
    #[inline]
    pub fn remove_int(&mut self, key: i64) -> Option<Value> {
        let position = self.find_int(key)?;
        self.remove_at(position).1
    }

    /// Unsets the live entry with non-numeric interned string key `key`.
    #[inline]
    pub fn remove_interned(&mut self, key: &'static InternedStr) -> Option<Value> {
        let position = self.find_interned(key)?;
        self.remove_at(position).1
    }

    /// Unsets the live entry with non-numeric string key bytes `bytes`.
    #[inline]
    pub fn remove_str_bytes(&mut self, bytes: &[u8]) -> Option<Value> {
        let position = self.find_str_bytes(bytes)?;
        self.remove_at(position).1
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
        };
        for (position, entry) in entries.iter().enumerate() {
            if entry.value.is_some() {
                index.insert(entries, position);
            }
        }
        Some(Box::new(index))
    }

    #[inline]
    fn find(&self, entries: &[Entry], key: &ArrayKey) -> Option<usize> {
        let short = short_hash(hash_array_key(key));
        self.positions
            .find(spread(short), |slot| {
                slot.hash == short && entries[slot.position()].key == *key
            })
            .map(|slot| slot.position())
    }

    #[inline]
    fn find_int(&self, entries: &[Entry], key: i64) -> Option<usize> {
        let short = short_hash(hash_int_key(key));
        self.positions
            .find(spread(short), |slot| {
                slot.hash == short
                    && matches!(entries[slot.position()].key, ArrayKey::Int(k) if k == key)
            })
            .map(|slot| slot.position())
    }

    #[inline]
    fn find_interned(&self, entries: &[Entry], key: &'static InternedStr) -> Option<usize> {
        let short = short_hash(key.array_hash());
        self.positions
            .find(spread(short), |slot| {
                slot.hash == short
                    && match &entries[slot.position()].key {
                        ArrayKey::Interned(k) => std::ptr::eq(*k, key),
                        ArrayKey::String(k) => k.as_bytes() == key.as_bytes(),
                        ArrayKey::Int(_) => false,
                    }
            })
            .map(|slot| slot.position())
    }

    #[inline]
    fn find_str_bytes(&self, entries: &[Entry], bytes: &[u8]) -> Option<usize> {
        let short = short_hash(hash_str_bytes(bytes));
        self.positions
            .find(spread(short), |slot| {
                slot.hash == short
                    && match &entries[slot.position()].key {
                        ArrayKey::Interned(k) => k.as_bytes() == bytes,
                        ArrayKey::String(k) => k.as_bytes() == bytes,
                        ArrayKey::Int(_) => false,
                    }
            })
            .map(|slot| slot.position())
    }

    fn insert(&mut self, entries: &[Entry], position: usize) {
        let short = short_hash(hash_array_key(&entries[position].key));
        self.positions
            .insert_unique(spread(short), Slot::new(position, short), |slot| {
                slot.table_hash()
            });
    }

    /// Drops the key from the index and returns where its entry is.
    fn remove(&mut self, entries: &[Entry], key: &ArrayKey) -> Option<usize> {
        let short = short_hash(hash_array_key(key));
        let (slot, _) = self
            .positions
            .find_entry(spread(short), |slot| {
                slot.hash == short && entries[slot.position()].key == *key
            })
            .ok()?
            .remove();
        Some(slot.position())
    }

    /// Drops the live entry at `position` from the index. It compares
    /// positions, not keys, which is cheaper.
    fn remove_position(&mut self, entries: &[Entry], position: usize) {
        let short = short_hash(hash_array_key(&entries[position].key));
        self.positions
            .find_entry(spread(short), |slot| slot.position() == position)
            .expect("every live entry is in the index")
            .remove();
    }
}
