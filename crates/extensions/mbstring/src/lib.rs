//! Multibyte text operations: `mb_strlen("é") === 1`.
use phera_extension_api::{Extension, Registry};

/// PHP's request-local multibyte string extension.
pub struct Mbstring;

impl Extension for Mbstring {
    fn name(&self) -> &'static str {
        "mbstring"
    }

    fn register(&self, _registry: &mut Registry) {}
}
