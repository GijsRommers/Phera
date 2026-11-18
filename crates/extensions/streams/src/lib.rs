//! PHP files and streams.
//!
//! ```php
//! $file = fopen('report.txt', 'rb');
//! echo fread($file, 5);
//! fclose($file);
//! ```

mod context_options;
mod copy;
mod csv;
mod data_uri;
mod directory;
mod directory_class;
mod errors;
mod file_attributes;
mod formatted;
mod glob;
mod handles;
mod include_path;
mod io_policy;
mod locking;
mod metadata;
mod mode;
mod mutations;
mod path_text;
mod pattern;
mod request;
mod resource;
mod scan;
mod scan_format;
mod stream;
mod stream_copy;
mod stream_options;
mod temporary;
mod transfer;
mod whole_file;
mod whole_write;
mod wrapper;

pub use request::{set_request, track_standard};
pub use whole_write::append_log;
pub use wrapper::standard_constants;

use phera_extension_api::{Extension, Registry};

pub struct Streams;

impl Extension for Streams {
    fn name(&self) -> &'static str {
        "streams"
    }

    fn php_module(&self) -> Option<&'static str> {
        None
    }

    fn register(&self, registry: &mut Registry) {
        context_options::register(registry);
        copy::register(registry);
        csv::register(registry);
        directory::register(registry);
        directory_class::register(registry);
        file_attributes::register(registry);
        formatted::register(registry);
        glob::register(registry);
        handles::register(registry);
        include_path::register(registry);
        locking::register(registry);
        metadata::register(registry);
        mutations::register(registry);
        path_text::register(registry);
        pattern::register(registry);
        resource::register(registry);
        scan::register(registry);
        stream_copy::register(registry);
        stream_options::register(registry);
        temporary::register(registry);
        transfer::register(registry);
        whole_file::register(registry);
    }
}
