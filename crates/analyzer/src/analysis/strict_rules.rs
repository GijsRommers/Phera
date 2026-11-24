//! Optional checks selected by the PHPStan strict-rules include.

mod foreach_overwrite;
mod loop_scope;
pub(super) use loop_scope::{foreach_definitions, remember_foreach_definitions};
