//! Dropping without recursion.
//!
//! `$a = []; for ($i = 0; $i < 200000; $i++) { $a = [$a]; }` nests arrays
//! 200,000 deep. Rust's own drop would recurse once per level and overflow
//! the stack, so an array that owns arrays, objects or references empties
//! them into a work list instead.

use super::PhpArray;
use super::storage::Storage;
use crate::destruction::Destruction;
use crate::value::Value;
use std::rc::Rc;

impl PhpArray {
    /// Moves this array's values into `work`, leaving it empty, so that
    /// dropping it afterwards does nothing more. They go in last first, so
    /// that `pop` frees them in order: PHP destroys an array's elements
    /// first to last, which decides the order of their destructors.
    pub(crate) fn take_values(&mut self, work: &mut Vec<Value>) {
        match std::mem::take(&mut self.storage) {
            Storage::List(values) => work.extend(values.into_iter().rev().flatten()),
            Storage::Map(map) => work.extend(
                map.entries
                    .into_iter()
                    .rev()
                    .filter_map(|entry| entry.value),
            ),
        }
    }

    /// Drops the values one by one. An array, object or reference that this
    /// was the last owner of has its own values added to the list.
    pub(crate) fn drop_values(mut work: Vec<Value>) {
        Self::drain_values(&mut work);
    }

    fn drain_values(work: &mut Vec<Value>) {
        while let Some(value) = work.pop() {
            Self::release_value(value, work);
        }
    }

    fn release_value(value: Value, work: &mut Vec<Value>) {
        match value {
            Value::Array(owner) => {
                if let Ok(mut array) = Rc::try_unwrap(owner) {
                    array.take_values(work);
                }
            }
            Value::Object(owner) => {
                // An object that waits for its destructor is handed to
                // the interpreter whole by its own `Drop`.
                if let Ok(object) = Rc::try_unwrap(owner)
                    && object.destruction() != Destruction::Due
                {
                    object.properties.take().take_values(work);
                    crate::lazy_object::release_native(object.native.take(), work);
                }
            }
            Value::Reference(owner) => {
                if let Ok(cell) = Rc::try_unwrap(owner) {
                    work.push(cell.into_inner());
                }
            }
            scalar => drop(scalar),
        }
    }

    pub(crate) fn release_entries(values: impl Iterator<Item = Value>) {
        let mut work = Vec::new();
        for value in values {
            match value {
                Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {}
                Value::String(string) => drop(string),
                Value::Resource(resource) => drop(resource),
                nested => {
                    Self::release_value(nested, &mut work);
                    Self::drain_values(&mut work);
                }
            }
        }
    }

    // Keep the iterative walker and its work list off the stack of small drops.
    #[inline(never)]
    fn release_storage(&mut self) {
        match std::mem::take(&mut self.storage) {
            Storage::List(values) => Self::release_entries(values.into_iter().flatten()),
            Storage::Map(map) => {
                Self::release_entries(map.entries.into_iter().filter_map(|entry| entry.value))
            }
        }
    }
}

impl Drop for PhpArray {
    fn drop(&mut self) {
        match &self.storage {
            Storage::List(values) if values.is_empty() => {}
            // Small property maps can use ordinary field drop if their entries cannot nest.
            Storage::Map(map)
                if map.index.is_none()
                    && !map.entries.iter().any(|entry| {
                        matches!(
                            entry.value,
                            Some(Value::Array(_) | Value::Object(_) | Value::Reference(_))
                        )
                    }) => {}
            _ => self.release_storage(),
        }
    }
}
