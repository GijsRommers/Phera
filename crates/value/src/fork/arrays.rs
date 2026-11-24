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
    fn new(
        array: Rc<PhpArray>,
        pure: Option<&foldhash::HashSet<usize>>,
        done: &foldhash::HashMap<usize, Rc<PhpArray>>,
    ) -> Self {
        let children = array
            .values()
            .filter_map(|value| match value {
                Value::Array(child) => {
                    let addr = address(child);
                    if pure.is_some_and(|set| set.contains(&addr)) || done.contains_key(&addr) {
                        None
                    } else {
                        Some(child.clone())
                    }
                }
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
        let root_addr = address(root);
        let pure = self.pure_arrays;
        if pure.is_some_and(|set| set.contains(&root_addr)) {
            return root.clone();
        }
        if let Some(done) = self.arrays.get(&root_addr) {
            return done.clone();
        }
        let mut has_nested_array = false;
        let mut has_identity = false;
        for value in root.values() {
            match value {
                Value::Array(child) => {
                    let child_addr = address(child);
                    if pure.is_some_and(|set| set.contains(&child_addr)) {
                        continue;
                    }
                    if let Some(done) = self.arrays.get(&child_addr) {
                        if !Rc::ptr_eq(done, child) {
                            has_identity = true;
                        }
                        continue;
                    }
                    has_nested_array = true;
                    break;
                }
                Value::Object(_) | Value::Reference(_) | Value::Resource(_) => {
                    has_identity = true;
                }
                _ => {}
            }
        }
        if !has_nested_array {
            let copy = if !has_identity {
                root.clone()
            } else {
                Rc::new(root.clone_mapped(|value| {
                    if needs_copy(value) {
                        self.value(value)
                    } else {
                        value.clone()
                    }
                }))
            };
            self.arrays.insert(root_addr, copy.clone());
            return copy;
        }
        let mut stack = vec![Visit::new(root.clone(), pure, &self.arrays)];
        let mut last_copy = None;
        while let Some(visit) = stack.last_mut() {
            if let Some(child) = visit.children.pop() {
                let child_addr = address(&child);
                if !pure.is_some_and(|set| set.contains(&child_addr))
                    && !self.arrays.contains_key(&child_addr)
                {
                    stack.push(Visit::new(child, pure, &self.arrays));
                }
                continue;
            }
            let Some(visit) = stack.pop() else { break };
            let copy = self.copy_array(&visit.array);
            self.arrays.insert(address(&visit.array), copy.clone());
            if stack.is_empty() {
                last_copy = Some(copy);
            }
        }
        last_copy.unwrap_or_else(|| root.clone())
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
        Rc::new(array.clone_mapped(|value| {
            if needs_copy(value) {
                self.value(value)
            } else {
                value.clone()
            }
        }))
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
