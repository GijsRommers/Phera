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

/// Cached per-function Tier-2 eligibility flags for one compiled unit revision,
/// computed once when the worker first numbers that revision so subsequent web
/// requests pay zero bytecode scanning cost in `CodeTable::load`.
#[derive(Clone, Default)]
pub(crate) enum FastCandidates {
    /// No function in this unit is a Tier-2 candidate.
    #[default]
    None,
    /// Bitmask for units with at most 64 functions (`main` + declared functions).
    Small(u64),
    /// Packed bitset for units with more than 64 functions: bit `n % 64`
    /// of word `n / 64` is function offset `n`.
    Large(Rc<[u64]>),
}

impl FastCandidates {
    #[cold]
    #[inline(never)]
    fn scan(unit: &CompiledUnit) -> Self {
        let total = 1 + unit.functions.len();
        let mut words = vec![0u64; total.div_ceil(64)];
        let mut mark = |offset: usize| words[offset / 64] |= 1u64 << (offset % 64);
        if crate::fast_tier::lower::is_fast_candidate(&unit.main, None) {
            mark(0);
        }
        for (idx, func) in unit.functions.iter().enumerate() {
            if crate::fast_tier::lower::is_fast_candidate(func, Some(idx as u32)) {
                mark(idx + 1);
            }
        }
        match words.as_slice() {
            _ if words.iter().all(|&word| word == 0) => Self::None,
            [mask] => Self::Small(*mask),
            _ => Self::Large(Rc::from(words)),
        }
    }

    #[inline(always)]
    pub(crate) fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline(always)]
    pub(crate) fn get(&self, offset: usize) -> bool {
        match self {
            Self::None => false,
            Self::Small(mask) => offset < 64 && (mask >> offset) & 1 != 0,
            Self::Large(words) => words
                .get(offset / 64)
                .is_some_and(|word| (word >> (offset % 64)) & 1 != 0),
        }
    }
}

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
    /// Cached Tier-2 eligibility flags per `UnitMetadata::id`.
    fast_candidates: Vec<FastCandidates>,
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
    #[cfg(test)]
    pub(crate) fn number(&self, unit: &Arc<CompiledUnit>, count: u32, leases: &Leases) -> u32 {
        self.number_with_fast(unit, count, leases).0
    }

    /// Like `number`, also returning cached per-function Tier-2 eligibility
    /// flags computed once per file revision.
    pub(crate) fn number_with_fast(
        &self,
        unit: &Arc<CompiledUnit>,
        count: u32,
        leases: &Leases,
    ) -> (u32, FastCandidates) {
        let mut numbering = self.numbering.borrow_mut();
        let Some(metadata) = &unit.metadata else {
            let first = numbering.take(count);
            leases.ranges.borrow_mut().push((first, count));
            return (first, FastCandidates::None);
        };
        let id = metadata.id as usize;
        if let Some(first) = numbering
            .firsts
            .get(id)
            .and_then(|first| first.checked_sub(1))
        {
            return (first, numbering.fast_candidates[id].clone());
        }
        numbering.give_back_dead_revisions();
        let first = numbering.take(count);
        if numbering.firsts.len() <= id {
            numbering.firsts.resize(id + 1, 0);
            numbering
                .fast_candidates
                .resize(id + 1, FastCandidates::None);
        }
        let candidates = FastCandidates::scan(unit);
        numbering.firsts[id] = first + 1;
        numbering.fast_candidates[id] = candidates.clone();
        numbering.revisions.push(Revision {
            id: metadata.id,
            unit: Arc::downgrade(unit),
            range: (first, count),
        });
        (first, candidates)
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
            if let Some(candidates) = self.fast_candidates.get_mut(dead.id as usize) {
                *candidates = FastCandidates::None;
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

    #[test]
    fn caches_fast_candidates_per_revision_and_resets_on_reclaim() {
        let worker = Rc::new(WorkerCode::default());
        let leases = Leases::new(&worker);
        let plain = unit("<?php function f() { return 1; }");
        let (first, fc1) = worker.number_with_fast(&plain, 2, &leases);
        assert!(fc1.is_none());
        let (second, fc2) = worker.number_with_fast(&plain, 2, &leases);
        assert_eq!(first, second);
        assert!(fc2.is_none());

        let hot = unit(
            "<?php function sum($n) { $s = 0; for ($i = 0; $i < $n; $i++) { $s += $i; } return $s; }",
        );
        let (hot_first, hot_fc1) = worker.number_with_fast(&hot, 2, &leases);
        assert!(!hot_fc1.get(0));
        assert!(hot_fc1.get(1));
        let (hot_second, hot_fc2) = worker.number_with_fast(&hot, 2, &leases);
        assert_eq!(hot_first, hot_second);
        assert!(!hot_fc2.get(0));
        assert!(hot_fc2.get(1));
    }
}
