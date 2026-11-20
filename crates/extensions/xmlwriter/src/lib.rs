//! Write XML incrementally to memory, a local URI, or a PHP stream.
//!
//! ```php
//! $writer = XMLWriter::toMemory();
//! $writer->startDocument();
//! $writer->writeElement('root', 'value');
//! $writer->endDocument();
//! echo $writer->outputMemory();
//! ```

mod bytes;
mod dtd;
mod factory;
mod operations;
mod output;
mod registration;
mod signatures_document;
mod signatures_markup;
mod writer;

use phera_extension_api::{Extension, Registry};

pub struct XmlWriter;

impl Extension for XmlWriter {
    fn name(&self) -> &'static str {
        "xmlwriter"
    }

    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}
