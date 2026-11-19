//! Dropping without recursion.
//!
//! `$a = []; for ($i = 0; $i < 200000; $i++) { $a = [$a]; }` nests arrays
//! 200,000 deep. Rust's own drop would recurse once per level and overflow
//! the stack, so an array that owns arrays, objects or references empties
//! them into a work list instead.

use super::PhpArray;
use super::storage::Storage;
use crate::value::Value;
use std::rc::Rc;

impl PhpArray {
    /// Moves this array's values into `work`, leaving it empty, so that
    /// dropping it afterwards does nothing more.
    fn take_values(&mut self, work: &mut Vec<Value>) {
        match std::mem::take(&mut self.storage) {
            Storage::List(values) => work.extend(values.into_iter().flatten()),
            Storage::Map(map) => {
                work.extend(map.entries.into_iter().filter_map(|entry| entry.value))
            }
        }
    }

    /// Drops the values one by one. An array, object or reference that this
    /// was the last owner of has its own values added to the list.
    pub(crate) fn drop_values(mut work: Vec<Value>) {
        while let Some(value) = work.pop() {
            match value {
                Value::Array(owner) => {
                    if let Ok(mut array) = Rc::try_unwrap(owner) {
                        array.take_values(&mut work);
                    }
                }
                Value::Object(owner) => {
                    if let Ok(object) = Rc::try_unwrap(owner) {
                        object.properties.into_inner().take_values(&mut work);
                        crate::lazy_object::release_native(object.native.into_inner(), &mut work);
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
    }
}

impl Drop for PhpArray {
    /// An array of scalars and strings cannot nest, so it skips the work
    /// list. The check scans the values: any of them may have been replaced
    /// through `get_mut` or `iter_mut`, so a flag kept on insert would not do.
    fn drop(&mut self) {
        if self.values().any(|value| {
            matches!(
                value,
                Value::Array(_) | Value::Object(_) | Value::Reference(_)
            )
        }) {
            let mut work = Vec::new();
            self.take_values(&mut work);
            Self::drop_values(work);
        }
    }
}
