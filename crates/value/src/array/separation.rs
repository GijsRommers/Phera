//! Array separation keeps shared PHP references but copies an unaliased value.
//!
//! ```php
//! $a = ['GET'];
//! $v =& $a[0];
//! unset($v);
//! $b = $a;
//! $b[0] = 'POST'; // $a still contains 'GET'
//! ```

use std::rc::Rc;

use super::storage::{Entry, OrderedMap, Storage};
use super::{ArrayKey, PhpArray};
use crate::value::Value;

impl Clone for PhpArray {
    fn clone(&self) -> Self {
        let storage = match &self.storage {
            Storage::List(values) => {
                Storage::List(values.iter().map(|value| self.copy_slot(value)).collect())
            }
            Storage::Map(map) => Storage::Map(OrderedMap {
                entries: map
                    .entries
                    .iter()
                    .map(|entry| Entry {
                        key: entry.key.clone(),
                        value: self.copy_slot(&entry.value),
                    })
                    .collect(),
                index: map.index.clone(),
            }),
        };
        Self {
            storage,
            next_free: self.next_free,
            dead: self.dead,
            pos: self.pos,
        }
    }
}

impl PhpArray {
    fn copy_slot(&self, slot: &Option<Value>) -> Option<Value> {
        slot.as_ref().map(|value| self.copy_element(value))
    }

    fn copy_element(&self, value: &Value) -> Value {
        if let Value::Reference(reference) = value
            && Rc::strong_count(reference) == 1
            && let Ok(inner) = reference.try_borrow()
            && !matches!(&*inner, Value::Array(array) if std::ptr::eq(array.as_ref(), self))
        {
            return inner.clone();
        }
        value.clone()
    }

    /// A new binding to an unaliased element must be installed in the
    /// writable array, which may still share its storage with a by-value copy.
    pub fn get_for_reference_binding(&self, key: &ArrayKey) -> Option<Value> {
        self.get(key).map(|value| self.copy_element(value))
    }
}
