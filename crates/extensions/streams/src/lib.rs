//! PHP files and streams.
//!
//! ```php
//! $file = fopen('report.txt', 'rb');
//! echo fread($file, 5);
//! fclose($file);
//! ```

use phera_extension_api::{Extension, Registry};

pub struct Streams;

impl Extension for Streams {
    fn name(&self) -> &'static str {
        "streams"
    }

    fn register(&self, _registry: &mut Registry) {}
}
