//! Flatten nested ownership into a work list only when recursive drop is possible.

use super::PhpArray;
use super::storage::Storage;
use crate::value::Value;
use std::rc::Rc;

impl PhpArray {
    /// Empty the owner before dropping it, so its own Drop cannot recurse.
    fn take_values(&mut self, work: &mut Vec<Value>) {
        match std::mem::take(&mut self.storage) {
            Storage::List(values) => work.extend(values.into_iter().flatten()),
            Storage::Map(map) => {
                work.extend(map.entries.into_iter().filter_map(|entry| entry.value))
            }
        }
    }

    /// Last-owner arrays, objects and references are drained from a loop, not the stack.
    fn drop_values(mut work: Vec<Value>) {
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
    /// Scalar and string arrays need no work list; mutable access makes scanning necessary.
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
