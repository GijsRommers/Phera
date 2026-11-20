//! A class's methods by interned name, for the method calls the interpreter
//! loop makes itself.
//!
//! ```php
//! $point->add(1, 2);   // the compiler wrote "add" as a name of the file,
//!                      // the code table gave it the number 42,
//!                      // and Point's table maps 42 to Point::add
//! ```
//!
//! The table is a small open-addressing hash table on those numbers, built
//! the first time a call looks a method up in the class. Looking a method up
//! is a multiplication, a mask and usually one comparison. A name no loaded
//! file has used has no number, so such a method is not in the table; calls
//! that miss take the general lookup, which finds every method.

use std::rc::{Rc, Weak};

use super::{Class, Method};
use crate::code_table::{CodeTable, NameKey};

/// One method of a class, with the class that declares it.
#[derive(Clone)]
struct MethodSlot {
    key: NameKey,
    method: Rc<Method>,
    /// The declaring class: the method's class scope. Weak, because a class
    /// is often the declaring class of its own methods.
    scope: Weak<Class>,
}

/// A method found in the table.
pub(crate) struct FoundMethod<'a> {
    pub(crate) method: &'a Rc<Method>,
    pub(crate) scope: &'a Weak<Class>,
}

/// The methods of one class by name key.
#[derive(Clone)]
pub(crate) struct MethodTable {
    /// A power-of-two number of slots, at most half of them in use.
    slots: Box<[Option<MethodSlot>]>,
}

impl MethodTable {
    /// Number every method of `class` whose name some loaded file uses.
    /// `scope_of` finds the class that declares a method.
    pub(crate) fn build(
        class: &Class,
        code: &CodeTable,
        scope_of: impl Fn(&Method) -> Option<Rc<Class>>,
    ) -> Self {
        // A direct trait call emits a deprecation every time. Composed
        // methods have their own table on the consuming class.
        if class.kind == phera_bytecode::ClassKind::Trait {
            return Self {
                slots: Box::new([None]),
            };
        }
        let size = (class.methods.len() * 2).next_power_of_two().max(4);
        let mut slots: Box<[Option<MethodSlot>]> = (0..size).map(|_| None).collect();
        for (name, method) in &class.methods {
            let Some(key) = code.name_key(name) else {
                continue;
            };
            let scope = scope_of(method).map_or_else(Weak::new, |scope| Rc::downgrade(&scope));
            let mut index = start(key, size);
            while slots[index].is_some() {
                index = (index + 1) & (size - 1);
            }
            slots[index] = Some(MethodSlot {
                key,
                method: method.clone(),
                scope,
            });
        }
        Self { slots }
    }

    /// The method with this name key, if the class has one.
    #[inline]
    pub(crate) fn get(&self, key: NameKey) -> Option<FoundMethod<'_>> {
        let mask = self.slots.len() - 1;
        let mut index = start(key, self.slots.len());
        loop {
            let slot = self.slots[index].as_ref()?;
            if slot.key == key {
                return Some(FoundMethod {
                    method: &slot.method,
                    scope: &slot.scope,
                });
            }
            index = (index + 1) & mask;
        }
    }
}

/// Where the search for `key` starts in a table of `size` slots.
#[inline]
fn start(key: NameKey, size: usize) -> usize {
    (key.0.wrapping_mul(0x9E37_79B9) >> 16) as usize & (size - 1)
}
