//! Runtime loading and include-once history belong to the current PHP request.

use std::rc::Rc;

use foldhash::fast::RandomState;
use indexmap::IndexSet;

use crate::{CodeLoader, IncludeCachePolicy, code_table::FunctionIndex};

type IncludedSet = IndexSet<String, RandomState>;

/// The runtime supplies code; the VM remembers which files have executed.
///
/// A forked request shares the snapshot's included-file set through `base`
/// without copying its paths, recording any request-local includes in `added`.
#[derive(Clone)]
pub(crate) struct IncludeState {
    loader: Option<Rc<dyn CodeLoader>>,
    pub(crate) policy: IncludeCachePolicy,
    pub(crate) stream_wrappers: bool,
    base: Rc<IncludedSet>,
    added: IncludedSet,
    entry_script: Option<FunctionIndex>,
}

impl IncludeState {
    pub(crate) fn new() -> Self {
        Self {
            loader: None,
            policy: IncludeCachePolicy::default(),
            stream_wrappers: false,
            base: Rc::new(IncludedSet::default()),
            added: IncludedSet::default(),
            entry_script: None,
        }
    }

    /// Copy for a snapshot or a request forked from one.
    ///
    /// When taking the snapshot, `added` is flattened into `base` once so
    /// every subsequent fork shares the entire set via an O(1) `Rc::clone`.
    pub(crate) fn forked(&self) -> Self {
        let base = if self.added.is_empty() {
            self.base.clone()
        } else {
            let mut merged = IncludedSet::with_capacity_and_hasher(
                self.base.len() + self.added.len(),
                RandomState::default(),
            );
            merged.extend(self.base.iter().cloned());
            merged.extend(self.added.iter().cloned());
            Rc::new(merged)
        };
        Self {
            loader: self.loader.clone(),
            policy: self.policy,
            stream_wrappers: self.stream_wrappers,
            base,
            added: IncludedSet::default(),
            entry_script: self.entry_script,
        }
    }

    pub(crate) fn set_loader(&mut self, loader: Rc<dyn CodeLoader>) {
        self.loader = Some(loader);
    }

    /// Clone the runtime handle so an include may await loading without borrowing state.
    pub(crate) fn loader(&self) -> Option<Rc<dyn CodeLoader>> {
        self.loader.clone()
    }

    pub(crate) fn entry_script(&self) -> Option<FunctionIndex> {
        self.entry_script
    }

    pub(crate) fn record_entry(&mut self, main: FunctionIndex, file: String) {
        self.entry_script = Some(main);
        self.record(file);
    }

    pub(crate) fn files(&self) -> impl Iterator<Item = &String> {
        self.base.iter().chain(self.added.iter())
    }

    pub(crate) fn contains(&self, file: &str) -> bool {
        self.base.contains(file) || self.added.contains(file)
    }

    pub(crate) fn record(&mut self, file: String) {
        if !self.base.contains(&file) {
            self.added.insert(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::IncludeState;

    #[test]
    fn forked_shares_snapshot_base_and_isolates_request_includes() {
        let mut boot = IncludeState::new();
        boot.record("a.php".into());
        boot.record("b.php".into());

        // Snapshot creation flattens boot's added files into a shared Rc base.
        let snapshot = boot.forked();
        assert!(snapshot.added.is_empty());

        let mut req1 = snapshot.forked();
        let req2 = snapshot.forked();
        assert!(Rc::ptr_eq(&snapshot.base, &req1.base));
        assert!(Rc::ptr_eq(&snapshot.base, &req2.base));

        req1.record("c.php".into());
        assert!(req1.contains("a.php"));
        assert!(req1.contains("c.php"));
        assert!(!req2.contains("c.php"));
        assert!(Rc::ptr_eq(&snapshot.base, &req1.base));
        let files: Vec<&str> = req1.files().map(String::as_str).collect();
        assert_eq!(files, vec!["a.php", "b.php", "c.php"]);
    }
}
