//! Runtime loading and include-once history belong to the current PHP request.

use std::rc::Rc;

use indexmap::IndexSet;

use crate::{CodeLoader, IncludeCachePolicy, code_table::FunctionIndex};

/// The runtime supplies code; the VM remembers which files have executed.
#[derive(Clone)]
pub(crate) struct IncludeState {
    loader: Option<Rc<dyn CodeLoader>>,
    pub(crate) policy: IncludeCachePolicy,
    pub(crate) stream_wrappers: bool,
    included: IndexSet<String>,
    entry_script: Option<FunctionIndex>,
}

impl IncludeState {
    pub(crate) fn new() -> Self {
        Self {
            loader: None,
            policy: IncludeCachePolicy::default(),
            stream_wrappers: false,
            included: IndexSet::new(),
            entry_script: None,
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
        self.included.iter()
    }

    pub(crate) fn contains(&self, file: &str) -> bool {
        self.included.contains(file)
    }

    pub(crate) fn record(&mut self, file: String) {
        self.included.insert(file);
    }
}
