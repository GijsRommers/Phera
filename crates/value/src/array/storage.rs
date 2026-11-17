//! The values-only list and the ordered map with a positions-only index.

use foldhash::fast::RandomState;
use hashbrown::HashTable;
use std::hash::BuildHasher;

use super::ArrayKey;
use crate::value::Value;

pub(super) const SCAN_LIMIT: usize = 8;

#[derive(Clone)]
pub(super) struct Entry {
    pub key: ArrayKey,
    pub value: Option<Value>,
}

/// Keep entry vectors inline; only large maps allocate separate index metadata.
#[derive(Clone)]
pub(super) enum Storage {
    List(Vec<Option<Value>>),
    Map(OrderedMap),
}

impl Default for Storage {
    fn default() -> Self {
        Self::List(Vec::new())
    }
}

#[derive(Clone)]
pub(super) struct OrderedMap {
    pub entries: Vec<Entry>,
    pub index: Option<Box<Index>>,
}

/// Small maps need neither an index allocation nor a per-array hasher.
#[derive(Clone)]
pub(super) struct Index {
    pub positions: HashTable<u32>,
    pub hasher: RandomState,
}

impl Storage {
    #[inline]
    pub fn len(&self) -> usize {
        match self {
            Self::List(values) => values.len(),
            Self::Map(map) => map.entries.len(),
        }
    }

    #[inline]
    pub fn value(&self, position: usize) -> Option<&Value> {
        match self {
            Self::List(values) => values.get(position)?.as_ref(),
            Self::Map(map) => map.entries.get(position)?.value.as_ref(),
        }
    }

    #[inline]
    pub fn slot_mut(&mut self, position: usize) -> Option<&mut Option<Value>> {
        match self {
            Self::List(values) => values.get_mut(position),
            Self::Map(map) => map.entries.get_mut(position).map(|entry| &mut entry.value),
        }
    }

    /// Retain every position, including holes: a foreach may still hold one.
    pub fn promote(&mut self) {
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
    }

    /// After explicit compaction or shift, a sequential map needs no stored keys.
    pub fn demote(&mut self) {
        if let Self::Map(map) = self {
            let sequential = map.entries.iter().enumerate().all(|(position, entry)| {
                entry.value.is_some() && entry.key == ArrayKey::Int(position as i64)
            });
            if sequential {
                *self = Self::List(
                    std::mem::take(&mut map.entries)
                        .into_iter()
                        .map(|entry| entry.value)
                        .collect(),
                );
            }
        }
    }
}

impl OrderedMap {
    /// Build an index only when scanning the entries would cost more.
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut map = Self {
            entries,
            index: None,
        };
        map.reindex();
        map
    }

    /// The table stores positions only; equality and rehashing consult the sole key copy.
    pub fn reindex(&mut self) {
        self.index = if self.entries.len() <= SCAN_LIMIT {
            None
        } else {
            let hasher = RandomState::default();
            let mut positions = HashTable::with_capacity(self.entries.len());
            for (position, entry) in self.entries.iter().enumerate() {
                if entry.value.is_some() {
                    positions.insert_unique(
                        hasher.hash_one(&entry.key),
                        u32::try_from(position).expect("array position fits u32"),
                        |index| hasher.hash_one(&self.entries[*index as usize].key),
                    );
                }
            }
            Some(Box::new(Index { positions, hasher }))
        };
    }

    /// Insert after the entry exists so a growing table can rehash through it.
    pub fn append(&mut self, key: ArrayKey, value: Value) {
        let position = u32::try_from(self.entries.len()).expect("array position fits u32");
        self.entries.push(Entry {
            key,
            value: Some(value),
        });
        match &mut self.index {
            Some(index) => {
                let hash = index.hasher.hash_one(&self.entries[position as usize].key);
                index.positions.insert_unique(hash, position, |position| {
                    index.hasher.hash_one(&self.entries[*position as usize].key)
                });
            }
            None if self.entries.len() > SCAN_LIMIT => self.reindex(),
            None => {}
        }
    }

    /// Remove the position before clearing its key, so lookups never see a dead entry.
    pub fn remove_index(&mut self, key: &ArrayKey) {
        if let Some(index) = &mut self.index {
            let hash = index.hasher.hash_one(key);
            if let Ok(entry) = index.positions.find_entry(hash, |position| {
                self.entries[*position as usize].key == *key
            }) {
                entry.remove();
            }
        }
    }
}
