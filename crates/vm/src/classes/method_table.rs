//! Inherited method slots keep `Child::run()` at its parent's slot number.
//!
//! For `class Child extends Parent { function run() {} }`, linking replaces
//! the parent's run slot. Lookup by a loaded Name searches only integer keys.

use super::Method;
use crate::names::Name;
use std::rc::Rc;

/// An inherited virtual method's stable position within a class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MethodSlot(pub(crate) u32);

/// A method address is copyable and carries the entry facts needed before frame entry.
#[derive(Clone, Copy)]
pub(crate) struct MethodAddress {
    pub(crate) class: super::class_table::ClassIndex,
    pub(crate) slot: MethodSlot,
    pub(crate) function: Option<crate::code_table::FunctionIndex>,
    pub(crate) is_static: bool,
}

/// The slot array preserves inheritance; sorted integer keys locate unknown receivers.
#[derive(Clone, Default)]
pub(crate) struct MethodTable {
    entries: Vec<(String, Rc<Method>)>,
    names: Vec<(Name, MethodSlot)>,
}

impl MethodTable {
    /// Replace overrides in place and append only newly declared method names.
    pub(crate) fn insert(&mut self, name: Name, spelling: String, method: Rc<Method>) {
        match self.names.binary_search_by_key(&name, |(name, _)| *name) {
            Ok(position) => self.entries[self.names[position].1.0 as usize].1 = method,
            Err(position) => {
                let slot = MethodSlot(self.entries.len() as u32);
                self.entries.push((spelling, method));
                self.names.insert(position, (name, slot));
            }
        }
    }

    /// Runtime dispatch never hashes or compares the source spelling.
    pub(crate) fn find(&self, name: Name) -> Option<MethodSlot> {
        self.names
            .binary_search_by_key(&name, |(name, _)| *name)
            .ok()
            .map(|position| self.names[position].1)
    }

    /// A resolved slot addresses inherited metadata directly.
    pub(crate) fn at(&self, slot: MethodSlot) -> &Rc<Method> {
        &self.entries[slot.0 as usize].1
    }

    /// Linking and reflection accept spellings; ordinary calls use find instead.
    pub(crate) fn get(&self, spelling: &str) -> Option<&Rc<Method>> {
        self.entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(spelling))
            .map(|(_, method)| method)
    }

    /// Reflection enumerates declaration metadata without cloning it.
    pub(crate) fn values(&self) -> impl Iterator<Item = &Rc<Method>> {
        self.entries.iter().map(|(_, method)| method)
    }

    /// Interface checks retain source spellings solely for diagnostics.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Rc<Method>)> {
        self.entries.iter().map(|(name, method)| (name, method))
    }

    /// Existence checks used by reflection share the declaration table.
    pub(crate) fn contains_key(&self, spelling: &str) -> bool {
        self.get(spelling).is_some()
    }
}
