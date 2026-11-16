//! Request-local include state state kept separate from register execution.

use std::sync::Arc;

use crate::CodeLoader;

/// Owns the request state for include state.
pub(crate) struct IncludeState {
    /// Request-owned loader.
    pub(crate) loader: Option<Arc<dyn CodeLoader>>,
    /// Request-owned included.
    pub(crate) included: std::collections::HashSet<String>,
}

impl IncludeState {
    pub(crate) fn new() -> Self {
        Self {
            loader: None,
            included: std::collections::HashSet::new(),
        }
    }
}
