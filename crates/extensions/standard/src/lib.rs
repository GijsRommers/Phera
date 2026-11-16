//! PHP's `ext/standard`: string, math and array functions.

mod arrays;
mod constants;
mod math;
mod strings;

use phera_extension_api::{Extension, Registry};

pub struct Standard;

impl Extension for Standard {
    fn name(&self) -> &'static str {
        "standard"
    }

    fn register(&self, registry: &mut Registry) {
        strings::register(registry);
        math::register(registry);
        arrays::register(registry);
        constants::register(registry);
    }
}
