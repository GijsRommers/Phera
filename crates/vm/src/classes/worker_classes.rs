//! The linked classes of a worker, one table shared by its requests.
//!
//! ```php
//! // first request:  new Post;   links Post (and Model, its traits) once
//! // next requests:  new Post;   finds the linked Post and declares it
//! ```
//!
//! An entry is an immutable linked class at a stable position: its number
//! is the same in every request of the worker. A request holds which
//! entries it has declared (`ClassTable`), never the worker table alone:
//! whether a name is a class in a request is that request's answer.
//!
//! A request that declares a class reuses an entry made from the same
//! declaration (the same file revision, the same class in it) when every
//! class the entry's linking looked up by name, found or not (its parent,
//! interfaces and traits, and the classes its method signature checks
//! named), is the same class in that request. Otherwise it links the class
//! itself and adds an entry. Entries whose file revision nothing holds any
//! more are dropped with that revision's function numbers.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use super::Class;

/// A class name looked up while an entry was linked, and what it was then:
/// the class's number, or `None` when no class had the name.
pub(crate) type Dependency = (String, Option<u32>);

/// A trait method composed into the class: its own function number, the
/// body it was composed from, and the name it has in the class.
pub(crate) type Import = (u32, u32, String);

/// What a request needs to declare an entry it did not link itself.
pub(crate) struct EntryFacts {
    pub(crate) class: Rc<Class>,
    pub(crate) lowercase_name: String,
    pub(crate) dependencies: Vec<Dependency>,
    pub(crate) imports: Vec<Import>,
}

#[derive(Default)]
pub(crate) struct WorkerClasses {
    inner: RefCell<Inner>,
}

/// The entries a file revision made: (file revision, class number in it,
/// entry position) each.
type RevisionEntries = (Weak<phera_bytecode::CompiledUnit>, Vec<(u32, u32, u32)>);

#[derive(Default)]
struct Inner {
    /// Every entry by position; `None` for a position reserved by a link
    /// that did not finish, or freed.
    entries: Vec<Option<Rc<EntryFacts>>>,
    /// The entries made from each declaration: file revision (its process
    /// number) and class number in the file.
    declarations: HashMap<(u32, u32), Vec<u32>>,
    /// Entries by file revision, to drop them when the revision is gone.
    revisions: Vec<RevisionEntries>,
}

impl WorkerClasses {
    /// A new position for a class being linked.
    pub(crate) fn reserve(&self) -> u32 {
        let mut inner = self.inner.borrow_mut();
        inner.entries.push(None);
        (inner.entries.len() - 1) as u32
    }

    /// The entries made from declaration `class` of `unit`, newest first.
    pub(crate) fn candidates(
        &self,
        unit: &Arc<phera_bytecode::CompiledUnit>,
        class: u32,
    ) -> Vec<Rc<EntryFacts>> {
        let Some(metadata) = &unit.metadata else {
            return Vec::new();
        };
        let inner = self.inner.borrow();
        inner
            .declarations
            .get(&(metadata.id, class))
            .into_iter()
            .flatten()
            .rev()
            .filter_map(|position| inner.entries.get(*position as usize)?.clone())
            .collect()
    }

    /// Keeps a class a request linked at `position`, for every request of
    /// the worker that declares the same class the same way.
    pub(crate) fn add(
        &self,
        unit: &Arc<phera_bytecode::CompiledUnit>,
        declaration: u32,
        position: u32,
        facts: EntryFacts,
    ) {
        let Some(metadata) = &unit.metadata else {
            return;
        };
        let mut inner = self.inner.borrow_mut();
        inner.drop_dead_revisions();
        let key = (metadata.id, declaration);
        inner.declarations.entry(key).or_default().push(position);
        if let Some(slot) = inner.entries.get_mut(position as usize) {
            *slot = Some(Rc::new(facts));
        }
        match inner
            .revisions
            .iter_mut()
            .find(|(revision, _)| revision.as_ptr() == Arc::as_ptr(unit))
        {
            Some((_, entries)) => entries.push((key.0, key.1, position)),
            None => inner
                .revisions
                .push((Arc::downgrade(unit), vec![(key.0, key.1, position)])),
        }
    }
}

impl Inner {
    /// Forgets the entries of file revisions nothing holds any more.
    fn drop_dead_revisions(&mut self) {
        let mut position = 0;
        while position < self.revisions.len() {
            if self.revisions[position].0.strong_count() > 0 {
                position += 1;
                continue;
            }
            let (_, entries) = self.revisions.swap_remove(position);
            for (unit, class, entry) in entries {
                self.declarations.remove(&(unit, class));
                if let Some(slot) = self.entries.get_mut(entry as usize) {
                    *slot = None;
                }
            }
        }
    }
}
