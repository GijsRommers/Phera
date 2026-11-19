//! Walking an array in order: `foreach ($a as $k => $v)`.
//!
//! Keys come back owned. A list stores no keys, so its keys are made from
//! positions; a map's keys are cloned, which for a string key is a
//! reference count, not a copy of the bytes.
//!
//! A `foreach` in the VM walks by position (`first_position`,
//! `next_position`, `entry_at`) rather than holding an iterator, because the
//! loop body may change the array it walks.

use super::storage::{Entry, Storage};
use super::{ArrayKey, PhpArray};
use crate::value::Value;

impl Storage {
    /// The slots of both shapes as two slices, one of them empty. Chaining
    /// an iterator over each gives one iterator type for either shape.
    pub(super) fn parts(&self) -> (&[Option<Value>], &[Entry]) {
        match self {
            Self::List(values) => (values, &[]),
            Self::Map(map) => (&[], &map.entries),
        }
    }

    fn parts_mut(&mut self) -> (&mut [Option<Value>], &mut [Entry]) {
        match self {
            Self::List(values) => (values, &mut []),
            Self::Map(map) => (&mut [], &mut map.entries),
        }
    }
}

impl PhpArray {
    pub fn iter(&self) -> impl Iterator<Item = (ArrayKey, &Value)> {
        let (list, entries) = self.storage.parts();
        let list = list.iter().enumerate().filter_map(|(position, value)| {
            value
                .as_ref()
                .map(|value| (ArrayKey::Int(position as i64), value))
        });
        let map = entries
            .iter()
            .filter_map(|entry| entry.value.as_ref().map(|value| (entry.key.clone(), value)));
        list.chain(map)
    }

    /// Values can change in place; keys and positions cannot.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ArrayKey, &mut Value)> {
        let (list, entries) = self.storage.parts_mut();
        let list = list.iter_mut().enumerate().filter_map(|(position, value)| {
            value
                .as_mut()
                .map(|value| (ArrayKey::Int(position as i64), value))
        });
        let map = entries
            .iter_mut()
            .filter_map(|entry| entry.value.as_mut().map(|value| (entry.key.clone(), value)));
        list.chain(map)
    }

    pub fn keys(&self) -> impl Iterator<Item = ArrayKey> {
        self.iter().map(|(key, _)| key)
    }

    /// Cheaper than `iter` when the keys are not needed: no key is made or
    /// cloned.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        let (list, entries) = self.storage.parts();
        list.iter()
            .flatten()
            .chain(entries.iter().filter_map(|entry| entry.value.as_ref()))
    }

    #[inline(always)]
    pub fn first(&self) -> Option<(ArrayKey, &Value)> {
        self.entry_at(self.first_position()?)
    }

    #[inline(always)]
    pub fn last(&self) -> Option<(ArrayKey, &Value)> {
        self.entry_at(self.last_position()?)
    }

    /// Position of the first live slot. A plain scan: written as
    /// `live_from(0)` it costs the positional foreach one more instruction
    /// per step (measured, 381 million against 401 million for 20 million
    /// steps).
    #[inline(always)]
    pub fn first_position(&self) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => values.iter().position(Option::is_some),
            Storage::Map(map) => map.entries.iter().position(|entry| entry.value.is_some()),
        }
    }

    /// Position of the last live slot.
    #[inline(always)]
    pub(super) fn last_position(&self) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => values.iter().rposition(Option::is_some),
            Storage::Map(map) => map.entries.iter().rposition(|entry| entry.value.is_some()),
        }
    }

    /// The next physical slot, including holes left by previously removed entries.
    pub fn end_position(&self) -> usize {
        self.storage.len()
    }

    /// Position of the first live slot after `pos`. `pos` may be stale: the
    /// array may have been compacted since, so it is not trusted to be in
    /// range.
    #[inline(always)]
    pub fn next_position(&self, pos: usize) -> Option<usize> {
        self.live_from(pos.checked_add(1)?)
    }

    /// Position of the first live slot at or after `start`.
    #[inline(always)]
    pub(super) fn live_from(&self, start: usize) -> Option<usize> {
        // Without holes every slot is live; most arrays have none.
        if self.dead == 0 {
            return (start < self.storage.len()).then_some(start);
        }
        let offset = match &self.storage {
            Storage::List(values) => values.get(start..)?.iter().position(Option::is_some),
            Storage::Map(map) => map
                .entries
                .get(start..)?
                .iter()
                .position(|entry| entry.value.is_some()),
        }?;
        Some(start + offset)
    }

    /// The key and value at `pos`, or `None` for a hole or past the end.
    #[inline(always)]
    pub fn entry_at(&self, pos: usize) -> Option<(ArrayKey, &Value)> {
        match &self.storage {
            Storage::List(values) => Some((ArrayKey::Int(pos as i64), values.get(pos)?.as_ref()?)),
            Storage::Map(map) => {
                let entry = map.entries.get(pos)?;
                Some((entry.key.clone(), entry.value.as_ref()?))
            }
        }
    }

    /// Like [`PhpArray::entry_at`], with the value writable. The key is a
    /// copy, so the stored key cannot change.
    #[inline(always)]
    pub fn entry_at_mut(&mut self, pos: usize) -> Option<(ArrayKey, &mut Value)> {
        match &mut self.storage {
            Storage::List(values) => {
                Some((ArrayKey::Int(pos as i64), values.get_mut(pos)?.as_mut()?))
            }
            Storage::Map(map) => {
                let entry = map.entries.get_mut(pos)?;
                Some((entry.key.clone(), entry.value.as_mut()?))
            }
        }
    }
}
