//! Identifiers are assigned numbers when code is loaded.
//!
//! `class A { function run() {} }` and `$a->RUN()` use the same Name; a
//! dynamically computed `$a->$name()` interns its value at the dynamic boundary.

use std::collections::HashMap;

/// A request-local identifier number, independent of source-unit name numbering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Name(pub(crate) u32);

/// Only loading and computed names consult the spelling map.
#[derive(Default)]
pub(crate) struct Names {
    identifiers: HashMap<String, Name>,
}

impl Names {
    /// PHP function, class and method identifiers are case insensitive.
    pub(crate) fn intern(&mut self, spelling: &str) -> Name {
        let lower = spelling.trim_start_matches('\\').to_ascii_lowercase();
        let next = Name(self.identifiers.len() as u32);
        *self.identifiers.entry(lower).or_insert(next)
    }

    /// Cold existence checks do not create declarations or new identifier numbers.
    pub(crate) fn find(&self, spelling: &str) -> Option<Name> {
        let lower = spelling.trim_start_matches('\\').to_ascii_lowercase();
        self.identifiers.get(&lower).copied()
    }
}
