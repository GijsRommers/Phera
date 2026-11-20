//! Identify a file's content, for example `(new finfo(FILEINFO_MIME_TYPE))->file('photo')`.

mod classifier;
mod handle;

use phera_extension_api::{Extension, Registry};

pub struct FileInfo;

impl Extension for FileInfo {
    fn name(&self) -> &'static str {
        "fileinfo"
    }

    fn register(&self, registry: &mut Registry) {
        handle::register(registry);
    }
}
