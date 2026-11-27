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

use super::key::numeric_string_key;
use super::lookup::list_position;
use super::storage::{Entry, Storage, list_index, list_key};
use super::{ArrayKey, PhpArray};
use crate::error::OpError;
use crate::intern::InternedStr;
use crate::string::PhpString;
use crate::value::Value;

/// Result of [`PhpArray::add_matching_map_entries_from`].
pub enum MapAddOutcome {
    /// Processed every live entry in `src`. `last` is `(last_pos, last_sum)` if non-empty.
    Done {
        last: Option<(usize, i64)>,
        count: usize,
    },
    /// Stopped early on a missing/non-int/ref/overflowing entry or work budget.
    Partial {
        last: Option<(usize, i64)>,
        count: usize,
        yielded: bool,
    },
}

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
        let (list_start, storage) = Storage::from_entries(storage);
        array.list_start = list_start;
        array.storage = storage;
        Ok(array)
    }

    /// Bulk keyed-foreach integer accumulation: for each live entry `(k, v)` of
    /// `src` (a `Storage::Map`), adds `v` into `self[k]` when `self[k]` already
    /// exists as an `Int` (fast-pathing matching slot indices with zero hashing
    /// and zero key clones). Stops cleanly before any missing key, non-int,
    /// reference, overflow, or budget limit. Starts at entry position `from`
    /// of `src`, which is where a run that stopped at its budget goes on.
    pub fn add_matching_map_entries_from(
        &mut self,
        src: &PhpArray,
        from: usize,
        budget: usize,
    ) -> MapAddOutcome {
        let Storage::Map(src_map) = &src.storage else {
            return MapAddOutcome::Partial {
                last: None,
                count: 0,
                yielded: false,
            };
        };
        if !matches!(self.storage, Storage::Map(_)) {
            return MapAddOutcome::Partial {
                last: None,
                count: 0,
                yielded: false,
            };
        }

        let mut last: Option<(usize, i64)> = None;
        let mut count: usize = 0;

        for (pos, src_entry) in src_map.entries.iter().enumerate().skip(from) {
            let Some(src_val) = &src_entry.value else {
                continue;
            };
            let Value::Int(rv) = *src_val else {
                return MapAddOutcome::Partial {
                    last,
                    count,
                    yielded: false,
                };
            };

            // Fast path: aligned insertion order (`dst.entries[pos].key == src.entries[pos].key`)
            // skips hashing, probing, and key cloning completely!
            let aligned_res = if let Storage::Map(dst_map) = &mut self.storage
                && let Some(dst_entry) = dst_map.entries.get_mut(pos)
                && dst_entry.key == src_entry.key
            {
                match &mut dst_entry.value {
                    Some(Value::Int(lv)) => match lv.checked_add(rv) {
                        Some(sum) => {
                            *lv = sum;
                            Some(Some(sum))
                        }
                        None => Some(None),
                    },
                    _ => Some(None),
                }
            } else {
                None
            };

            let sum = match aligned_res {
                Some(Some(sum)) => sum,
                Some(None) => {
                    return MapAddOutcome::Partial {
                        last,
                        count,
                        yielded: false,
                    };
                }
                None => match self.get_mut(&src_entry.key) {
                    Some(Value::Int(lv)) => match lv.checked_add(rv) {
                        Some(sum) => {
                            *lv = sum;
                            sum
                        }
                        None => {
                            return MapAddOutcome::Partial {
                                last,
                                count,
                                yielded: false,
                            };
                        }
                    },
                    _ => {
                        return MapAddOutcome::Partial {
                            last,
                            count,
                            yielded: false,
                        };
                    }
                },
            };

            last = Some((pos, sum));
            count += 1;
            if count >= budget {
                return MapAddOutcome::Partial {
                    last,
                    count,
                    yielded: true,
                };
            }
        }

        MapAddOutcome::Done { last, count }
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
        let (list_start, storage) = Storage::from_entries(entries.storage);
        Self {
            storage,
            next_free: entries.next_free.unwrap_or(0),
            list_start,
            has_next_free: entries.next_free.is_some(),
            ..Self::default()
        }
    }

    /// Replaces the value of an existing key in place, or appends the entry
    /// at the end, even when the key was unset earlier. Returns the old value.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        if let ArrayKey::Int(index) = key {
            return self.insert_int(index, value);
        }
        let list_start = self.list_start;
        match &mut self.storage {
            Storage::List(_) => {
                self.list_start = 0;
                self.storage.as_map(list_start).append(key, value);
                self.maybe_compact();
                None
            }
            Storage::Map(map) => {
                let replaced = map.insert(key, value);
                if replaced.is_none() {
                    self.maybe_compact();
                }
                replaced
            }
        }
    }

    /// Negative keys count too, as in PHP 8.3 and later: after `[-5 => 1]`,
    /// `$a[]` uses -4.
    #[inline]
    fn note_int_key(&mut self, key: i64) {
        if !self.has_next_free || key >= self.next_free {
            self.set_next_free(key.saturating_add(1));
        }
    }

    /// `array_multisort()` starts appending at the row count, even for arrays
    /// that retained string keys.
    pub fn set_next_append_index_to_len(&mut self) {
        self.set_next_free(self.len() as i64);
    }

    /// Appends an entry whose key is not in the array. A list stays a list
    /// only when the key is the next position: a key that fills a hole or
    /// skips ahead goes at the end in PHP, so the list becomes a map.
    #[inline]
    fn push_entry(&mut self, key: ArrayKey, value: Value) {
        let list_start = self.list_start;
        match &mut self.storage {
            Storage::List(values) if values.is_empty() => {
                if let ArrayKey::Int(index) = key
                    && let Ok(base) = i32::try_from(index)
                {
                    self.list_start = base;
                    values.push(Some(value));
                } else {
                    self.list_start = 0;
                    self.storage.as_map(list_start).append(key, value);
                }
            }
            Storage::List(values) if key == ArrayKey::Int(list_key(list_start, values.len())) => {
                values.push(Some(value));
            }
            storage => {
                self.list_start = 0;
                storage.as_map(list_start).append(key, value);
            }
        }
        self.maybe_compact();
    }

    /// `$a[] = $value`: appends at the next free integer key. That key
    /// stops at `PHP_INT_MAX`, which can be used once: `$a[]` fails only
    /// when the entry `PHP_INT_MAX` exists.
    #[inline]
    pub fn push(&mut self, value: Value) -> Result<ArrayKey, OpError> {
        let key = self.next_free_index().unwrap_or(0);
        if key == i64::MAX && self.contains_key(&ArrayKey::Int(key)) {
            return Err(OpError::error(
                "Cannot add element to the array as the next element is already occupied",
            ));
        }
        self.set_next_free(key.saturating_add(1));
        self.push_entry(ArrayKey::Int(key), value);
        Ok(ArrayKey::Int(key))
    }

    /// Replaces the value of an existing integer key in place, or appends it.
    #[inline]
    pub fn insert_int(&mut self, index: i64, value: Value) -> Option<Value> {
        self.note_int_key(index);
        let list_start = self.list_start;
        match &mut self.storage {
            Storage::List(values) => {
                if values.is_empty() {
                    if let Ok(base) = i32::try_from(index) {
                        self.list_start = base;
                        values.push(Some(value));
                        return None;
                    }
                } else if let Some(position) = list_index(list_start, index) {
                    if let Some(slot) = values.get_mut(position) {
                        if let Some(existing) = slot.as_mut() {
                            return Some(std::mem::replace(existing, value));
                        }
                    } else if position == values.len() {
                        values.push(Some(value));
                        self.maybe_compact();
                        return None;
                    }
                }
                self.list_start = 0;
                self.storage
                    .as_map(list_start)
                    .append(ArrayKey::Int(index), value);
                self.maybe_compact();
                None
            }
            Storage::Map(map) => {
                let replaced = map.insert(ArrayKey::Int(index), value);
                if replaced.is_none() {
                    self.maybe_compact();
                }
                replaced
            }
        }
    }

    /// Single-probe integer insert or update that refuses to overwrite a PHP
    /// reference, returning `Err(value)` without mutating the array.
    #[inline]
    pub fn insert_int_if_not_ref(
        &mut self,
        index: i64,
        value: Value,
    ) -> Result<Option<Value>, Value> {
        let list_start = self.list_start;
        match &mut self.storage {
            Storage::List(values) => {
                if values.is_empty() {
                    if let Ok(base) = i32::try_from(index) {
                        self.list_start = base;
                        values.push(Some(value));
                        self.note_int_key(index);
                        return Ok(None);
                    }
                    self.note_int_key(index);
                    self.list_start = 0;
                    self.storage
                        .as_map(list_start)
                        .append(ArrayKey::Int(index), value);
                    return Ok(None);
                }
                if let Some(position) = list_index(list_start, index) {
                    if let Some(slot) = values.get_mut(position) {
                        if let Some(existing) = slot.as_mut() {
                            if existing.is_reference() {
                                return Err(value);
                            }
                            return Ok(Some(std::mem::replace(existing, value)));
                        }
                    } else if position == values.len() {
                        values.push(Some(value));
                        self.note_int_key(index);
                        self.maybe_compact();
                        return Ok(None);
                    }
                }
                self.note_int_key(index);
                self.list_start = 0;
                self.storage
                    .as_map(list_start)
                    .append(ArrayKey::Int(index), value);
                self.maybe_compact();
                Ok(None)
            }
            Storage::Map(map) => {
                let replaced = map.insert_if_not_ref(ArrayKey::Int(index), value)?;
                self.note_int_key(index);
                if replaced.is_none() {
                    self.maybe_compact();
                }
                Ok(replaced)
            }
        }
    }

    /// [`PhpArray::insert`] that refuses to overwrite a PHP reference,
    /// returning `Err(value)` without mutating the array. A new key costs one
    /// hash and one probe of the index, not a failed lookup and an insert.
    #[inline]
    pub fn insert_if_not_ref(
        &mut self,
        key: ArrayKey,
        value: Value,
    ) -> Result<Option<Value>, Value> {
        if let ArrayKey::Int(index) = key {
            return self.insert_int_if_not_ref(index, value);
        }
        let list_start = self.list_start;
        match &mut self.storage {
            Storage::List(_) => {
                self.list_start = 0;
                self.storage.as_map(list_start).append(key, value);
                self.maybe_compact();
                Ok(None)
            }
            Storage::Map(map) => {
                let replaced = map.insert_if_not_ref(key, value)?;
                if replaced.is_none() {
                    self.maybe_compact();
                }
                Ok(replaced)
            }
        }
    }

    /// `unset($a[$key])`: leaves a hole, so later slots keep their positions.
    /// It never compacts, because a running foreach may hold one of them.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let list_start = self.list_start;
        let value = match &mut self.storage {
            Storage::List(values) => values.get_mut(list_position(list_start, key)?)?.take()?,
            Storage::Map(map) => map.remove(key)?,
        };
        self.dead += 1;
        Some(value)
    }

    /// `unset($a[$int_key])` without constructing an `ArrayKey`.
    #[inline]
    pub fn remove_int(&mut self, key: i64) -> Option<Value> {
        let list_start = self.list_start;
        let value = match &mut self.storage {
            Storage::List(values) => values.get_mut(list_index(list_start, key)?)?.take()?,
            Storage::Map(map) => map.remove_int(key)?,
        };
        self.dead += 1;
        Some(value)
    }

    /// `unset($a[$interned_key])` without cloning an `ArrayKey`.
    #[inline]
    pub fn remove_interned(&mut self, key: &'static InternedStr) -> Option<Value> {
        if let Some(int_key) = key.numeric_key() {
            return self.remove_int(int_key);
        }
        let value = match &mut self.storage {
            Storage::List(_) => return None,
            Storage::Map(map) => map.remove_interned(key)?,
        };
        self.dead += 1;
        Some(value)
    }

    /// `unset($a[$str_key])` without cloning an `ArrayKey`.
    #[inline]
    pub fn remove_str(&mut self, key: &PhpString) -> Option<Value> {
        if let Some(int_key) = numeric_string_key(key.as_bytes()) {
            return self.remove_int(int_key);
        }
        let value = match &mut self.storage {
            Storage::List(_) => return None,
            Storage::Map(map) => map.remove_str_bytes(key.as_bytes())?,
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
        let list_start = self.list_start;
        let entries: Vec<Entry> = match std::mem::take(&mut self.storage) {
            Storage::List(values) => values
                .into_iter()
                .enumerate()
                .filter_map(|(position, value)| {
                    Some(Entry {
                        key: ArrayKey::Int(list_key(list_start, position)),
                        value: Some(value?),
                    })
                })
                .collect(),
            Storage::Map(mut map) => {
                map.entries.retain(|entry| entry.value.is_some());
                map.entries
            }
        };
        let (new_start, new_storage) = Storage::from_entries(entries);
        self.list_start = new_start;
        self.storage = new_storage;
        self.dead = 0;
        self.set_pointer(pointer);
    }

    /// `array_pop`: removes the last entry. The next free key goes back by
    /// one when the popped key was the last one handed out.
    pub fn pop(&mut self) -> Option<Value> {
        let position = self.last_position()?;
        let list_start = self.list_start;
        let (key, value) = match &mut self.storage {
            Storage::List(values) => (
                ArrayKey::Int(list_key(list_start, position)),
                values[position].take(),
            ),
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
            if self.storage.len() == 0 {
                self.list_start = 0;
            }
            self.dead -= holes_after as u32;
        }
        if let ArrayKey::Int(key) = key
            && self.next_free_index() == Some(key.saturating_add(1))
        {
            self.set_next_free(key);
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
        let (new_start, new_storage) = match std::mem::take(&mut self.storage) {
            Storage::List(mut values) => {
                values.retain(Option::is_some);
                next = values.len() as i64;
                (0, Storage::List(values))
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
        self.list_start = new_start;
        self.storage = new_storage;
        self.dead = 0;
        self.set_next_free(next);
        self.reset();
        value
    }
}
