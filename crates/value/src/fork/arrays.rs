//! Arrays in a fork: shared unless something inside must be copied.
//!
//! ```php
//! $routes = ['home' => '/', 'posts' => '/posts'];   // shared by every fork
//! $services = ['mailer' => new Mailer];             // copied: holds an object
//! $nested = ['a' => ['b' => [$counter = new C]]];   // copied, all three levels
//! ```
//!
//! An array is a value, so a fork may keep using the snapshot's array as
//! long as it holds nothing with identity: a write copies it first, as any
//! write to a shared array does. An array that holds an object or a
//! reference, directly or in a nested array, is rebuilt with the copies.
//! The rebuilt array is a clone of the original, so its internal pointer
//! and next free key stay as they were.
//!
//! Nested arrays are visited with an explicit stack, children before their
//! parent, and each distinct array (by address) is handled once, so arrays
//! that share storage still share it in the fork.

use std::rc::Rc;

use super::Fork;
use crate::array::PhpArray;
use crate::value::Value;

/// An array being visited, with the nested arrays still to visit.
struct Visit {
    array: Rc<PhpArray>,
    children: Vec<Rc<PhpArray>>,
}

impl Visit {
    fn new(array: Rc<PhpArray>) -> Self {
        let children = array
            .values()
            .filter_map(|value| match value {
                Value::Array(child) => Some(child.clone()),
                _ => None,
            })
            .collect();
        Visit { array, children }
    }
}

impl Fork<'_> {
    /// The fork's version of an array: the same `Rc` when nothing inside
    /// needs copying, otherwise a rebuilt clone.
    pub fn array(&mut self, root: &Rc<PhpArray>) -> Rc<PhpArray> {
        if let Some(done) = self.arrays.get(&address(root)) {
            return done.clone();
        }
        let mut stack = vec![Visit::new(root.clone())];
        while let Some(visit) = stack.last_mut() {
            if let Some(child) = visit.children.pop() {
                if !self.arrays.contains_key(&address(&child)) {
                    stack.push(Visit::new(child));
                }
                continue;
            }
            let Some(visit) = stack.pop() else { break };
            let copy = self.copy_array(&visit.array);
            self.arrays.insert(address(&visit.array), copy);
        }
        self.arrays
            .get(&address(root))
            .cloned()
            .unwrap_or_else(|| root.clone())
    }

    /// Replaces every value of `array` that has identity by its copy, in
    /// place: for an object's property table, which is not behind an `Rc`.
    pub(crate) fn array_contents(&mut self, array: &mut PhpArray) {
        let mut position = array.first_position();
        while let Some(at) = position {
            let original = array.entry_at(at).map(|(_, value)| value.clone());
            if let Some(original) = original
                && needs_copy(&original)
            {
                let copy = self.value(&original);
                if let Some((_, slot)) = array.entry_at_mut(at) {
                    *slot = copy;
                }
            }
            position = array.next_position(at);
        }
    }

    /// One array whose nested arrays have all been handled.
    fn copy_array(&mut self, array: &Rc<PhpArray>) -> Rc<PhpArray> {
        let changes = array.values().any(|value| match value {
            Value::Array(child) => self
                .arrays
                .get(&address(child))
                .is_some_and(|copy| !Rc::ptr_eq(copy, child)),
            Value::Object(_) | Value::Reference(_) | Value::Resource(_) => true,
            _ => false,
        });
        if !changes {
            return array.clone();
        }
        let mut copy = PhpArray::clone(array);
        self.array_contents(&mut copy);
        Rc::new(copy)
    }
}

/// Whether a value may differ in the fork: anything but a scalar or string.
pub(super) fn needs_copy(value: &Value) -> bool {
    matches!(
        value,
        Value::Array(_) | Value::Object(_) | Value::Reference(_) | Value::Resource(_)
    )
}

fn address(array: &Rc<PhpArray>) -> usize {
    Rc::as_ptr(array) as usize
}
