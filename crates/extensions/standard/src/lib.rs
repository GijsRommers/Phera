//! PHP's `ext/standard`: string, math, array and time functions.

use phera_extension_api::{Extension, Registry};

mod arrays;
mod constants;
mod math;
mod natural;
mod strings;
mod time;

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
        time::register(registry);
    }
}

/// Exposes PHP comparison traversal for adversarial comparator regression tests.
#[doc(hidden)]
pub use arrays::zend_sort::order as php_sort_order;

/// PHP argument errors name boolean literals rather than the general bool type.
pub(crate) fn argument_type_name(value: &phera_extension_api::Value) -> String {
    match value {
        phera_extension_api::Value::Bool(false) => "false".into(),
        phera_extension_api::Value::Bool(true) => "true".into(),
        _ => value.type_name(),
    }
}
