//! Functions every script expects: inspecting values (`var_dump`, `print_r`,
//! `var_export`), type tests and conversions, `count`, the engine's constants,
//! and the exception classes.

mod constants;
mod count;
mod dump;
mod interfaces;
mod throwable;
mod types;

use phera_extension_api::{Extension, Registry};

pub struct Core;

impl Extension for Core {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register(&self, registry: &mut Registry) {
        interfaces::register(registry);
        constants::register(registry);
        count::register(registry);
        dump::register(registry);
        throwable::register(registry);
        types::register(registry);
    }
}
