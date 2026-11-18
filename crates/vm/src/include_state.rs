//! Runtime loading and include-once history belong to the current PHP request.

use std::{collections::HashSet, sync::Arc};

use crate::CodeLoader;

/// The runtime supplies code; the VM remembers which files have executed.
pub(crate) struct IncludeState {
    loader: Option<Arc<dyn CodeLoader>>,
    included: HashSet<String>,
    path: Option<String>,
}

impl IncludeState {
    pub(crate) fn new() -> Self {
        Self {
            loader: None,
            included: HashSet::new(),
            path: None,
        }
    }

    pub(crate) fn set_loader(&mut self, loader: Arc<dyn CodeLoader>) {
        self.loader = Some(loader);
    }

    /// Clone the runtime handle so an include may await loading without borrowing state.
    pub(crate) fn loader(&self) -> Option<Arc<dyn CodeLoader>> {
        self.loader.clone()
    }

    pub(crate) fn path(&self) -> &str {
        self.path.as_deref().unwrap_or(".")
    }

    pub(crate) fn set_path(&mut self, path: String) -> String {
        self.path.replace(path).unwrap_or_else(|| ".".into())
    }

    pub(crate) fn contains(&self, file: &str) -> bool {
        self.included.contains(file)
    }

    pub(crate) fn record(&mut self, file: String) {
        self.included.insert(file);
    }
}
