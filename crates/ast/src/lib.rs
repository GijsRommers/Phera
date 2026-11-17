//! The syntax tree the parser produces and the compiler consumes.
//!
//! Every node carries the [`phera_source::Span`] it was parsed from. The tree
//! stays close to the source: names are kept as written and are resolved against
//! `namespace` and `use` by the compiler, not the parser.

mod declaration;
mod expression;
mod name;
mod program;
mod statement;

pub use declaration::*;
pub use expression::*;
pub use name::*;
pub use program::*;
pub use statement::*;
