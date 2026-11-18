//! PHP's `ext/standard`: string, math, array and time functions.

use phera_extension_api::{Extension, Registry};

mod arguments;
mod arrays;
mod constants;
mod math;
mod natural;
mod offsets;
mod path_metadata;
mod strings;
mod time;
mod version_compare;

pub struct Standard;

impl Extension for Standard {
    fn name(&self) -> &'static str {
        "standard"
    }

    fn register(&self, registry: &mut Registry) {
        registry.function(version_compare::ENTRY);
        for entry in path_metadata::FUNCTIONS {
            registry.function(*entry);
        }
        strings::register(registry);
        math::register(registry);
        arrays::register(registry);
        constants::register(registry);
        for entry in natural::FUNCTIONS.iter().chain(time::FUNCTIONS) {
            registry.function(*entry);
        }
    }
}

/// Exposes PHP comparison traversal for adversarial comparator regression tests.
#[doc(hidden)]
pub use arrays::zend_sort::order as php_sort_order;
