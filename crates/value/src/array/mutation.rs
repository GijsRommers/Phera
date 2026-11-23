//! Writing: `$a[$k] = $v`, `$a[] = $v`, `unset($a[$k])`, `array_pop`,
//! `array_shift`.
//!
//! ```php
//! $a = [5 => 'a'];
//! $a[] = 'b';        // key 6: one more than the largest integer key so far
//! unset($a[6]);
//! $a[] = 'c';        // key 7: unset does not give a key back
//! array_pop($a);
//! $a[] = 'd';        // key 7 again: pop does give it back
//! ```

use super::lookup::list_position;
use super::storage::{Entry, Storage};
use super::{ArrayKey, PhpArray};
use crate::error::OpError;
use crate::value::Value;

/// The entries of an array being built by [`PhpArray::build_distinct`].
pub struct DistinctEntries {
    storage: Vec<Entry>,
    next_free: Option<i64>,
}

impl DistinctEntries {
    /// Appends an entry whose key is not among those pushed before.
    #[inline]
    pub fn push(&mut self, key: ArrayKey, value: Value) {
        if let ArrayKey::Int(index) = key
            && self.next_free.is_none_or(|next| index >= next)
        {
            self.next_free = Some(index.saturating_add(1));
        }
        self.storage.push(Entry {
            key,
            value: Some(value),
        });
    }
}

impl PhpArray {
    /// Builds an ordered array from entries whose keys are already distinct.
    /// Callers must validate that property before using this constructor.
    pub fn try_from_distinct<E>(
        entries: impl IntoIterator<Item = Result<(ArrayKey, Value), E>>,
    ) -> Result<Self, E> {
        let entries = entries.into_iter();
        let capacity = entries.size_hint().0;
        Self::try_from_distinct_with_capacity(entries, capacity)
    }

    /// [`PhpArray::try_from_distinct`] for entries whose number the caller
    /// knows (or bounds closely): the slots are allocated once, at that
    /// size, where an iterator that filters reports none.
    pub fn try_from_distinct_with_capacity<E>(
        entries: impl IntoIterator<Item = Result<(ArrayKey, Value), E>>,
        capacity: usize,
    ) -> Result<Self, E> {
        let entries = entries.into_iter();
        let mut storage = Vec::with_capacity(capacity);
        let mut array = Self::new();
        for entry in entries {
            let (key, value) = entry?;
            if let ArrayKey::Int(index) = key {
                array.note_int_key(index);
            }
            storage.push(Entry {
                key,
                value: Some(value),
            });
        }
        array.storage = Storage::from_entries(storage);
        Ok(array)
    }

    /// Builds an ordered array from entries whose keys are already distinct,
    /// which `fill` pushes one by one into slots made for `capacity`. The
    /// entries go straight into the array's storage, with no iterator and no
    /// result in between.
    pub fn build_distinct(capacity: usize, fill: impl FnOnce(&mut DistinctEntries)) -> Self {
        let mut entries = DistinctEntries {
            storage: Vec::with_capacity(capacity),
            next_free: None,
        };
        fill(&mut entries);
        Self {
            storage: Storage::from_entries(entries.storage),
            next_free: entries.next_free,
            ..Self::default()
        }
    }

    /// Replaces the value of an existing key in place, or appends the entry
    /// at the end, even when the key was unset earlier. Returns the old value.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        if let ArrayKey::Int(index) = key {
            self.note_int_key(index);
        }
        if let Some(slot) = self.get_mut(&key) {
            return Some(std::mem::replace(slot, value));
        }
        self.push_entry(key, value);
        None
    }

    /// Negative keys count too, as in PHP 8.3 and later: after `[-5 => 1]`,
    /// `$a[]` uses -4.
    #[inline]
    fn note_int_key(&mut self, key: i64) {
        if self.next_free.is_none_or(|next| key >= next) {
            self.next_free = Some(key.saturating_add(1));
        }
    }

    /// `array_multisort()` starts appending at the row count, even for arrays
    /// that retained string keys.
    pub fn set_next_append_index_to_len(&mut self) {
        self.next_free = Some(self.len() as i64);
    }

    /// Appends an entry whose key is not in the array. A list stays a list
    /// only when the key is the next position: a key that fills a hole or
    /// skips ahead goes at the end in PHP, so the list becomes a map.
    #[inline]
    fn push_entry(&mut self, key: ArrayKey, value: Value) {
        match &mut self.storage {
            Storage::List(values) if key == ArrayKey::Int(values.len() as i64) => {
                values.push(Some(value));
            }
            storage => storage.as_map().append(key, value),
        }
        self.maybe_compact();
    }

    /// `$a[] = $value`: appends at the next free integer key. That key
    /// stops at `PHP_INT_MAX`, which can be used once: `$a[]` fails only
    /// when the entry `PHP_INT_MAX` exists.
    #[inline]
    pub fn push(&mut self, value: Value) -> Result<ArrayKey, OpError> {
        let key = self.next_free.unwrap_or(0);
        if key == i64::MAX && self.contains_key(&ArrayKey::Int(key)) {
            return Err(OpError::error(
                "Cannot add element to the array as the next element is already occupied",
            ));
        }
        self.next_free = Some(key.saturating_add(1));
        self.push_entry(ArrayKey::Int(key), value);
        Ok(ArrayKey::Int(key))
    }

    /// `unset($a[$key])`: leaves a hole, so later slots keep their positions.
    /// It never compacts, because a running foreach may hold one of them.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let value = match &mut self.storage {
            Storage::List(values) => values.get_mut(list_position(key)?)?.take()?,
            Storage::Map(map) => map.remove(key)?,
        };
        self.dead += 1;
        Some(value)
    }

    /// Drops the holes once they outnumber the live entries. Runs after an
    /// append or a pop, never after a plain unset (see [`PhpArray::remove`]).
    fn maybe_compact(&mut self) {
        if self.dead as usize <= self.len() {
            return;
        }
        let pointer = self.pointer_after_compaction();
        let entries: Vec<Entry> = match std::mem::take(&mut self.storage) {
            Storage::List(values) => values
                .into_iter()
                .enumerate()
                .filter_map(|(position, value)| {
                    Some(Entry {
                        key: ArrayKey::Int(position as i64),
                        value: Some(value?),
                    })
                })
                .collect(),
            Storage::Map(mut map) => {
                map.entries.retain(|entry| entry.value.is_some());
                map.entries
            }
        };
        self.storage = Storage::from_entries(entries);
        self.dead = 0;
        self.set_pointer(pointer);
    }

    /// `array_pop`: removes the last entry. The next free key goes back by
    /// one when the popped key was the last one handed out.
    pub fn pop(&mut self) -> Option<Value> {
        let position = self.last_position()?;
        let (key, value) = match &mut self.storage {
            Storage::List(values) => (ArrayKey::Int(position as i64), values[position].take()),
            Storage::Map(map) => map.remove_at(position),
        };
        // The slot goes, with the holes after it, so that the next `$a[]`
        // reuses it and a list stays a list. Not for a PHP reference: a
        // by-reference `foreach` binds the element it stands on, and those
        // it has visited, as references, and PHP's cursor on a removed slot
        // goes past it, so that slot must stay a hole (an element `$a[]`
        // adds then comes after it, where the loop still finds it).
        if matches!(value, Some(Value::Reference(_))) {
            self.dead += 1;
        } else {
            let holes_after = self.storage.len() - position - 1;
            self.storage.truncate(position);
            self.dead -= holes_after as u32;
        }
        if let ArrayKey::Int(key) = key
            && self.next_free == Some(key.saturating_add(1))
        {
            self.next_free = Some(key);
        }
        self.maybe_compact();
        value
    }

    /// `array_shift`: removes the first entry, drops every hole and numbers
    /// the integer keys again from 0. String keys keep their place.
    pub fn shift(&mut self) -> Option<Value> {
        let first = self.first_position()?;
        let value = self.storage.slot_mut(first)?.take();
        let mut next = 0;
        self.storage = match std::mem::take(&mut self.storage) {
            Storage::List(mut values) => {
                values.retain(Option::is_some);
                next = values.len() as i64;
                Storage::List(values)
            }
            Storage::Map(mut map) => {
                map.entries.retain(|entry| entry.value.is_some());
                for entry in &mut map.entries {
                    if let ArrayKey::Int(key) = &mut entry.key {
                        *key = next;
                        next += 1;
                    }
                }
                Storage::from_entries(map.entries)
            }
        };
        self.dead = 0;
        self.next_free = Some(next);
        self.reset();
        value
    }
}
