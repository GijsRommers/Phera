//! Owned keys let lists compute their keys without allocating backing storage.

use super::storage::Storage;
use super::{ArrayKey, PhpArray};
use crate::value::Value;

impl PhpArray {
    /// Compute list keys, cloning only the reference-counted keys of maps.
    pub fn iter(&self) -> impl Iterator<Item = (ArrayKey, &Value)> {
        let (list, map) = match &self.storage {
            Storage::List(values) => (Some(values), None),
            Storage::Map(map) => (None, Some(&map.entries)),
        };
        list.into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(position, value)| {
                value
                    .as_ref()
                    .map(|value| (ArrayKey::Int(position as i64), value))
            })
            .chain(
                map.into_iter().flatten().filter_map(|entry| {
                    entry.value.as_ref().map(|value| (entry.key.clone(), value))
                }),
            )
    }

    /// Mutating a value never changes its key or physical position.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (ArrayKey, &mut Value)> {
        let (list, map) = match &mut self.storage {
            Storage::List(values) => (Some(values), None),
            Storage::Map(map) => (None, Some(&mut map.entries)),
        };
        list.into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(position, value)| {
                value
                    .as_mut()
                    .map(|value| (ArrayKey::Int(position as i64), value))
            })
            .chain(
                map.into_iter().flatten().filter_map(|entry| {
                    entry.value.as_mut().map(|value| (entry.key.clone(), value))
                }),
            )
    }

    pub fn keys(&self) -> impl Iterator<Item = ArrayKey> {
        self.iter().map(|(key, _)| key)
    }

    /// Value-only walks avoid even incrementing string-key reference counts.
    pub fn values(&self) -> impl Iterator<Item = &Value> {
        let (list, map) = match &self.storage {
            Storage::List(values) => (Some(values), None),
            Storage::Map(map) => (None, Some(&map.entries)),
        };
        list.into_iter().flatten().filter_map(Option::as_ref).chain(
            map.into_iter()
                .flatten()
                .filter_map(|entry| entry.value.as_ref()),
        )
    }

    #[inline(always)]
    pub fn first(&self) -> Option<(ArrayKey, &Value)> {
        self.entry_at(self.first_position()?)
    }
    #[inline(always)]
    pub fn last(&self) -> Option<(ArrayKey, &Value)> {
        self.entry_at(self.last_position()?)
    }

    /// Positional iteration skips holes without changing the caller's stored position.
    #[inline(always)]
    pub fn first_position(&self) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => values.iter().position(Option::is_some),
            Storage::Map(map) => map.entries.iter().position(|entry| entry.value.is_some()),
        }
    }

    #[inline(always)]
    pub(super) fn last_position(&self) -> Option<usize> {
        match &self.storage {
            Storage::List(values) => values.iter().rposition(Option::is_some),
            Storage::Map(map) => map.entries.iter().rposition(|entry| entry.value.is_some()),
        }
    }

    /// Bounds-check the saved position: mutation may have compacted the array meanwhile.
    #[inline(always)]
    pub fn next_position(&self, pos: usize) -> Option<usize> {
        let start = pos.checked_add(1)?;
        // Dense arrays need no per-slot tag scan; most foreach loops stay dense.
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

    /// Return an owned key so a list never needs a backing key allocation.
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

    /// Borrow the slot alone; the returned key cannot change the stored key.
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
