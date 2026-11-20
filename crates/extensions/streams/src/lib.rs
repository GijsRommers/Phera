//! PHP files and streams.
//!
//! ```php
//! $file = fopen('report.txt', 'rb');
//! echo fread($file, 5);
//! fclose($file);
//! ```

mod child_pipe;
mod context_options;
mod copy;
mod crypto_constants;
mod csv;
mod data_uri;
mod directory;
mod directory_class;
mod errors;
mod file_attributes;
pub mod file_io;
mod formatted;
mod glob;
mod gzip;
pub use gzip::register as register_gzip_functions;
mod handles;
mod include_path;
mod ini;
mod input;
mod io_policy;
mod locking;
mod metadata;
mod mode;
mod mutations;
pub mod native;
mod path_text;
mod pattern;
mod request;
mod resource;
mod scan;
mod scan_format;
mod select;
mod source_tools;
mod stream;
mod stream_copy;
mod stream_options;
mod temporary;
mod terminal;
mod transfer;
mod user_file;
mod user_wrappers;
mod whole_file;
mod whole_write;
mod wrapper;

pub use child_pipe::{
    ChildStreamKind, CloseAttachment, child_stream, close_child_stream, duplicate_descriptor,
    open_descriptor, set_attachment, wait_with_execution_limit,
};
pub use input::RequestInput;
pub use path_text::parent as dirname_parent;
pub use request::{creation_mask, new_resource};
pub use request::{request_input, set_request, track_standard};
pub use terminal::{stream_kind, with_descriptor};
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
        crypto_constants::register(registry);
        copy::register(registry);
        csv::register(registry);
        directory::register(registry);
        directory_class::register(registry);
        file_attributes::register(registry);
        formatted::register(registry);
        glob::register(registry);
        handles::register(registry);
        include_path::register(registry);
        ini::register(registry);
        locking::register(registry);
        metadata::register(registry);
        mutations::register(registry);
        path_text::register(registry);
        pattern::register(registry);
        resource::register(registry);
        scan::register(registry);
        source_tools::register(registry);
        stream_copy::register(registry);
        stream_options::register(registry);
        select::register(registry);
        temporary::register(registry);
        transfer::register(registry);
        user_wrappers::register(registry);
        whole_file::register(registry);
    }
}
