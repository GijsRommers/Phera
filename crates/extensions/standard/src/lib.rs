//! PHP's `ext/standard`: string, math and array functions.

use phera_extension_api::{Extension, Registry};

mod arrays;
mod constants;
mod math;
mod natural;
mod strings;

pub struct Standard;

impl Extension for Standard {
    fn name(&self) -> &'static str {
        "standard"
    }

    fn register(&self, registry: &mut Registry) {
        strings::register(registry);
        math::register(registry);
        arrays::register(registry);
        natural::register(registry);
        constants::register(registry);
    }
}
