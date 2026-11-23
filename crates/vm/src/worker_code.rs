//! Function numbers that mean the same in every request of a worker.
//!
//! ```php
//! // app/Models/Post.php, loaded by the first request: Post::save is 4120
//! // the next request loads it after other files: Post::save is 4120 again
//! ```
//!
//! The worker numbers a file revision's functions the first time any of
//! its requests (or its boot) loads that revision, and keeps the numbers
//! while something holds the revision. A request's code table stays one
//! flat vector indexed by these numbers; only loading a file asks the
//! worker, never a call. A linked class can then name its methods by number
//! and be shared by every request of the worker.
//!
//! Numbers are reused, so that the table stays as large as the code that is
//! alive: code compiled without a file (`eval`) and the methods a class
//! composes from traits lease numbers for one request, and a file revision
//! that nothing holds any more (an edited file whose old compile no request
//! and no snapshot uses) gives its numbers back to the next revision.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Weak};

use phera_bytecode::CompiledUnit;

/// The worker's numbering, shared by its requests (see the module comment).
#[derive(Default)]
pub(crate) struct WorkerCode {
    numbering: RefCell<Numbering>,
}

#[derive(Default)]
struct Numbering {
    /// The first number of each numbered file revision plus one, by the
    /// revision's process number (`UnitMetadata::id`); 0: not numbered.
    firsts: Vec<u32>,
    /// The numbered revisions, to give back the numbers of those nothing
    /// holds any more.
    revisions: Vec<Revision>,
    /// The next number never given out.
    next: u32,
    /// Ranges given back: their first number and length.
    free: Vec<(u32, u32)>,
}

struct Revision {
    id: u32,
    unit: Weak<CompiledUnit>,
    range: (u32, u32),
}

/// Numbers one request (or boot) leased. They go back to the worker when
/// the last holder is gone: a snapshot's leases are shared by its forks,
/// which may outlive it when a new boot replaces it.
pub(crate) struct Leases {
    worker: Rc<WorkerCode>,
    ranges: RefCell<Vec<(u32, u32)>>,
}

impl Leases {
    pub(crate) fn new(worker: &Rc<WorkerCode>) -> Rc<Self> {
        Rc::new(Leases {
            worker: worker.clone(),
            ranges: RefCell::default(),
        })
    }
}

impl Leases {
    /// Keeps `number` given out for the life of the worker: a trait method
    /// composed into a class that every request of the worker shares.
    pub(crate) fn keep(&self, number: u32) {
        self.ranges
            .borrow_mut()
            .retain(|range| *range != (number, 1));
    }
}

impl Drop for Leases {
    fn drop(&mut self) {
        let mut numbering = self.worker.numbering.borrow_mut();
        numbering.free.append(self.ranges.get_mut());
    }
}

impl WorkerCode {
    /// The first number of `unit`'s `count` functions. A file revision from
    /// the code cache keeps its numbers for every request; other code leases
    /// them into `leases`.
    pub(crate) fn number(&self, unit: &Arc<CompiledUnit>, count: u32, leases: &Leases) -> u32 {
        let mut numbering = self.numbering.borrow_mut();
        let Some(metadata) = &unit.metadata else {
            let first = numbering.take(count);
            leases.ranges.borrow_mut().push((first, count));
            return first;
        };
        let id = metadata.id as usize;
        if let Some(first) = numbering
            .firsts
            .get(id)
            .and_then(|first| first.checked_sub(1))
        {
            return first;
        }
        numbering.give_back_dead_revisions();
        let first = numbering.take(count);
        if numbering.firsts.len() <= id {
            numbering.firsts.resize(id + 1, 0);
        }
        numbering.firsts[id] = first + 1;
        numbering.revisions.push(Revision {
            id: metadata.id,
            unit: Arc::downgrade(unit),
            range: (first, count),
        });
        first
    }

    /// One number for a request's own function (a method composed from a
    /// trait), leased into `leases`.
    pub(crate) fn lease_one(&self, leases: &Leases) -> u32 {
        let first = self.numbering.borrow_mut().take(1);
        leases.ranges.borrow_mut().push((first, 1));
        first
    }
}

impl Numbering {
    /// A range of `count` numbers: a free one that fits (the rest of it stays
    /// free), or new ones.
    fn take(&mut self, count: u32) -> u32 {
        if let Some(position) = self.free.iter().position(|(_, length)| *length >= count) {
            let (first, length) = self.free[position];
            if length == count {
                self.free.swap_remove(position);
            } else {
                self.free[position] = (first + count, length - count);
            }
            return first;
        }
        let first = self.next;
        self.next = self.next.saturating_add(count);
        first
    }

    /// Frees the numbers of revisions that nothing holds any more.
    fn give_back_dead_revisions(&mut self) {
        let mut position = 0;
        while position < self.revisions.len() {
            if self.revisions[position].unit.strong_count() > 0 {
                position += 1;
                continue;
            }
            let dead = self.revisions.swap_remove(position);
            if let Some(first) = self.firsts.get_mut(dead.id as usize) {
                *first = 0;
            }
            self.free.push(dead.range);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use super::{Leases, WorkerCode};

    fn unit(source: &str) -> Arc<phera_bytecode::CompiledUnit> {
        let mut unit =
            phera_compiler::compile_bytes(source.as_bytes(), "worker_code.php").expect("compiles");
        unit.metadata = Some(Arc::new(phera_bytecode::UnitMetadata::new(&unit)));
        Arc::new(unit)
    }

    #[test]
    fn a_file_keeps_its_numbers_and_leases_go_back() {
        let worker = Rc::new(WorkerCode::default());
        let (a, b) = (unit("<?php function a() {}"), unit("<?php function b() {}"));
        let first = Leases::new(&worker);
        let a_first = worker.number(&a, 2, &first);
        let b_first = worker.number(&b, 2, &first);
        let second = Leases::new(&worker);
        assert_eq!(
            worker.number(&b, 2, &second),
            b_first,
            "the same in every request"
        );
        assert_eq!(worker.number(&a, 2, &second), a_first);
        let leased = worker.lease_one(&second);
        drop(second);
        let third = Leases::new(&worker);
        assert_eq!(
            worker.lease_one(&third),
            leased,
            "a lease goes back with its request"
        );
    }

    #[test]
    fn a_revision_nothing_holds_gives_its_numbers_to_the_next() {
        let worker = Rc::new(WorkerCode::default());
        let leases = Leases::new(&worker);
        let old = unit("<?php function f() {} function g() {}");
        let old_first = worker.number(&old, 3, &leases);
        drop(old);
        let new = unit("<?php function f() {} function g() {}");
        assert_eq!(worker.number(&new, 3, &leases), old_first);
    }
}
