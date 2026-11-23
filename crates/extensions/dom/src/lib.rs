//! The arena behind PHP's DOM extension.
//!
//! ```php
//! $document = new DOMDocument();
//! $root = $document->appendChild($document->createElement('root'));
//! ```
//!
//! Parsing document strings never loads external entities, reads files or
//! makes network requests. The same restriction applies when callers set
//! entity substitution or DTD loading flags.

mod bindings;
pub use bindings::shared;
pub use bindings::{Dom, Libxml};

pub mod document;
pub mod html;
mod schema;
pub mod serialize;
pub mod tree;
pub mod xml;
pub mod xpath;
