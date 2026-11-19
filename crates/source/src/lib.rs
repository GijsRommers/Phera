//! Source-code identity and positions.

pub mod attributes;
pub mod deprecations;
mod diagnostic;
mod language_level;
mod source_file;
mod span;

pub use diagnostic::{CompileDiagnostic, CompileDiagnosticKind, CompileDiagnosticSite};
pub use language_level::{Availability, Change, LanguageLevel};
pub use source_file::LineTable;
pub use span::Span;
