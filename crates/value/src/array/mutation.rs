//! Mutation preserves PHP's next-free index, insertion order, and iteration holes.

use super::storage::Storage;
use super::{ArrayKey, INVALID, PhpArray};
use crate::error::OpError;
use crate::value::Value;

impl PhpArray {
    /// Replace in place, or append in insertion order even when reusing a deleted key.
    pub fn insert(&mut self, key: ArrayKey, value: Value) -> Option<Value> {
        if let ArrayKey::Int(index) = &key {
            self.note_int_key(*index);
        }
        if let Some(position) = self.find_pos(&key) {
            return self.storage.slot_mut(position)?.replace(value);
        }
        self.push_entry(key, value);
        None
    }

    /// Negative keys participate too, as in PHP 8.3 and later.
    fn note_int_key(&mut self, key: i64) {
        if self.next_free.is_none_or(|next| key >= next) {
            self.next_free = Some(key.saturating_add(1));
        }
    }

    /// A list can extend only at its physical end: reusing a hole changes order.
    fn push_entry(&mut self, key: ArrayKey, value: Value) {
        if let Storage::List(values) = &mut self.storage
            && key == ArrayKey::Int(values.len() as i64)
        {
            values.push(Some(value));
        } else {
            self.storage.promote();
            if let Storage::Map(map) = &mut self.storage {
                map.append(key, value);
            }
        }
        self.maybe_compact();
    }

    /// Append at PHP's next-free integer key, rejecting the saturated MAX state.
    pub fn push(&mut self, value: Value) -> Result<ArrayKey, OpError> {
        let key = self.next_free.unwrap_or(0);
        if self.next_free == Some(i64::MAX) {
            return Err(OpError::error(
                "Cannot add element to the array as the next element is already occupied",
            ));
        }
        self.note_int_key(key);
        self.push_entry(ArrayKey::Int(key), value);
        Ok(ArrayKey::Int(key))
    }

    /// Removal alone must never compact or truncate: foreach keeps physical positions.
    pub fn remove(&mut self, key: &ArrayKey) -> Option<Value> {
        let position = self.find_pos(key)?;
        if let Storage::Map(map) = &mut self.storage {
            map.remove_index(key);
        }
        let value = self.storage.slot_mut(position)?.take();
        self.dead += 1;
        value
    }

    /// Preserve the existing compaction boundary: after append/pop, when dead > live.
    fn maybe_compact(&mut self) {
        if self.dead as usize <= self.len() {
            return;
        }
        // A list's keys would change if its values alone were compacted.
        self.storage.promote();
        if let Storage::Map(map) = &mut self.storage {
            map.entries.retain(|entry| entry.value.is_some());
            map.reindex();
        }
        self.dead = 0;
        self.storage.demote();
    }

    /// Pop rolls the next-free key back only when the last key immediately precedes it.
    pub fn pop(&mut self) -> Option<Value> {
        let position = self.last_position()?;
        let (key, _) = self.entry_at(position)?;
        let value = self.remove(&key)?;
        if let (ArrayKey::Int(key), Some(next)) = (key, self.next_free)
            && next == key.saturating_add(1)
        {
            self.next_free = Some(key);
        }
        self.maybe_compact();
        Some(value)
    }

    /// Shift closes every hole and renumbers integers; string keys retain their order.
    pub fn shift(&mut self) -> Option<Value> {
        let first = self.first_position()?;
        let value = self.storage.slot_mut(first)?.take();
        let mut next = 0;
        match &mut self.storage {
            Storage::List(values) => {
                values.retain(Option::is_some);
                next = values.len() as i64;
            }
            Storage::Map(map) => {
                map.entries.retain(|entry| entry.value.is_some());
                for entry in &mut map.entries {
                    if matches!(entry.key, ArrayKey::Int(_)) {
                        entry.key = ArrayKey::Int(next);
                        next += 1;
                    }
                }
                map.reindex();
            }
        }
        self.storage.demote();
        self.dead = 0;
        self.next_free = Some(next);
        self.pos = self
            .first_position()
            .map_or(INVALID, |position| position as u32);
        value
    }
}
