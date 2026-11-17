//! PHP hashing: `hash('sha256', 'hello')` returns a lowercase hexadecimal digest.
use phera_extension_api::{Extension, Registry};
pub struct Hash;
impl Extension for Hash {
    fn name(&self) -> &'static str {
        "hash"
    }
    fn register(&self, _registry: &mut Registry) {}
}
