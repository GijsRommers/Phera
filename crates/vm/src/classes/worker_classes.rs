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
    /// Positions given back by requests, handed out again first.
    free: Vec<u32>,
    /// The first entry with each class name, by the name's process key,
    /// plus one (0: none), and the next entry with the same name, by
    /// position, plus one: every entry of a name in two flat arrays.
    heads: Vec<u32>,
    next: Vec<u32>,
    /// The key of each entry's lowercased name, by position, to unlink it.
    entry_keys: Vec<u32>,
    /// Lowercased class names to their process keys, for names a script
    /// builds at run time (`new $class`, `class_exists($name)`).
    keys: HashMap<Box<str>, phera_bytecode::NameKey>,
}

/// Positions one request (or boot) took for classes it linked for itself
/// (an enum, an anonymous class, a class whose linking raised a
/// diagnostic, a link that failed). They go back to the worker when the
/// last holder is gone: a snapshot's are shared by its forks.
pub(crate) struct PositionLeases {
    worker: Rc<WorkerClasses>,
    positions: RefCell<Vec<u32>>,
}

impl PositionLeases {
    pub(crate) fn new(worker: &Rc<WorkerClasses>) -> Rc<Self> {
        Rc::new(PositionLeases {
            worker: worker.clone(),
            positions: RefCell::default(),
        })
    }

    pub(crate) fn lease(&self, position: u32) {
        self.positions.borrow_mut().push(position);
    }
}

impl Drop for PositionLeases {
    fn drop(&mut self) {
        let mut inner = self.worker.inner.borrow_mut();
        inner.free.append(self.positions.get_mut());
    }
}

impl WorkerClasses {
    /// A new position for a class being linked.
    pub(crate) fn reserve(&self) -> u32 {
        let mut inner = self.inner.borrow_mut();
        if let Some(position) = inner.free.pop() {
            return position;
        }
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
        inner.chain(&facts.lowercase_name, position);
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

impl WorkerClasses {
    /// The first entry named by `key`, if any.
    #[inline]
    pub(crate) fn first(&self, key: phera_bytecode::NameKey) -> Option<u32> {
        let inner = self.inner.borrow();
        inner.heads.get(key.0 as usize)?.checked_sub(1)
    }

    /// The entry after `position` with the same name, if any.
    #[inline]
    pub(crate) fn after(&self, position: u32) -> Option<u32> {
        let inner = self.inner.borrow();
        inner.next.get(position as usize)?.checked_sub(1)
    }

    /// The process key of a lowercased class name some entry has.
    pub(crate) fn key_of(&self, lowercase: &str) -> Option<phera_bytecode::NameKey> {
        self.inner.borrow().keys.get(lowercase).copied()
    }
}

impl Inner {
    /// Puts `position` first among the entries named `lowercase`.
    fn chain(&mut self, lowercase: &str, position: u32) {
        let key = match self.keys.get(lowercase) {
            Some(key) => *key,
            None => {
                let key = phera_bytecode::name_keys::key(lowercase);
                self.keys.insert(lowercase.into(), key);
                key
            }
        };
        let (slot, at) = (key.0 as usize, position as usize);
        if self.heads.len() <= slot {
            self.heads.resize(slot + 1, 0);
        }
        if self.next.len() <= at {
            self.next.resize(at + 1, 0);
            self.entry_keys.resize(at + 1, u32::MAX);
        }
        self.next[at] = self.heads[slot];
        self.heads[slot] = position + 1;
        self.entry_keys[at] = key.0;
    }

    /// Takes `position` out of its name's chain.
    fn unchain(&mut self, position: u32) {
        let at = position as usize;
        let Some(key) = self
            .entry_keys
            .get(at)
            .copied()
            .filter(|key| *key != u32::MAX)
        else {
            return;
        };
        let after = self.next[at];
        if self.heads[key as usize] == position + 1 {
            self.heads[key as usize] = after;
        } else {
            let mut previous = self.heads[key as usize];
            while previous != 0 {
                let link = (previous - 1) as usize;
                if self.next[link] == position + 1 {
                    self.next[link] = after;
                    break;
                }
                previous = self.next[link];
            }
        }
        self.next[at] = 0;
        self.entry_keys[at] = u32::MAX;
    }

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
                self.unchain(entry);
                self.free.push(entry);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{PositionLeases, WorkerClasses};

    #[test]
    fn a_position_leased_by_a_request_is_handed_out_again() {
        let worker = Rc::new(WorkerClasses::default());
        let snapshot = PositionLeases::new(&worker);
        let kept = worker.reserve();
        snapshot.lease(kept);
        let fork = PositionLeases::new(&worker);
        let own = worker.reserve();
        fork.lease(own);
        drop(fork);
        assert_eq!(
            worker.reserve(),
            own,
            "a request's position goes back when it ends"
        );
        let shared_by_a_fork = snapshot.clone();
        drop(snapshot);
        assert_ne!(
            worker.reserve(),
            kept,
            "a snapshot's position stays while a fork holds it"
        );
        drop(shared_by_a_fork);
        assert_eq!(worker.reserve(), kept);
    }
}
