//! PHP's `ext/ctype`: character class tests.

use phera_extension_api::{Extension, Registry};

pub struct Ctype;

impl Extension for Ctype {
    fn name(&self) -> &'static str {
        "ctype"
    }

    fn register(&self, _registry: &mut Registry) {}
}
