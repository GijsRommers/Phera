//! Runtime loading and include-once history belong to the current PHP request.

use std::{collections::HashSet, sync::Arc};

use crate::CodeLoader;

/// The runtime supplies code; the VM remembers which files have executed.
pub(crate) struct IncludeState {
    loader: Option<Arc<dyn CodeLoader>>,
    included: HashSet<String>,
}

impl IncludeState {
    pub(crate) fn new() -> Self {
        Self {
            loader: None,
            included: HashSet::new(),
        }
    }

    pub(crate) fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.loader = Some(loader);
    }

    /// Clone the runtime handle so an include may await loading without borrowing state.
    pub(crate) fn loader(&self) -> Option<Arc<dyn CodeLoader>> {
        self.loader.clone()
    }

    pub(crate) fn contains(&self, file: &str) -> bool {
        self.included.contains(file)
    }

    pub(crate) fn record(&mut self, file: String) {
        self.included.insert(file);
    }
}
