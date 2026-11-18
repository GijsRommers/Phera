//! PHP files and streams.
//!
//! ```php
//! $file = fopen('report.txt', 'rb');
//! echo fread($file, 5);
//! fclose($file);
//! ```

mod context_options;
mod errors;
mod handles;
mod io_policy;
mod locking;
mod metadata;
mod mode;
mod request;
mod resource;
mod stream;
mod temporary;
mod transfer;
mod whole_file;
mod wrapper;

pub use request::set_request;
pub use wrapper::standard_constants;

use phera_extension_api::{Extension, Registry};

pub struct Streams;

impl Extension for Streams {
    fn name(&self) -> &'static str {
        "streams"
    }

    fn register(&self, registry: &mut Registry) {
        context_options::register(registry);
        handles::register(registry);
        locking::register(registry);
        metadata::register(registry);
        resource::register(registry);
        temporary::register(registry);
        transfer::register(registry);
        whole_file::register(registry);
    }
}
