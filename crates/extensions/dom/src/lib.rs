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

pub mod tree;
