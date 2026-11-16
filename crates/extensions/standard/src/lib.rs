//! PHP's `ext/standard`: string, math and array functions.

use phera_extension_api::{Extension, Registry};

pub struct Standard;

impl Extension for Standard {
    fn name(&self) -> &'static str {
        "standard"
    }

    fn register(&self, _registry: &mut Registry) {}
}
