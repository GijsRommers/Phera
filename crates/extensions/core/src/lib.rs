//! Functions every script expects: inspecting values (`var_dump`, `print_r`,
//! `var_export`), type tests and conversions, `count`, and the engine's
//! constants.

mod constants;
mod count;
mod dump;
mod types;

use phera_extension_api::{Extension, Registry};

pub struct Core;

impl Extension for Core {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        count::register(registry);
        dump::register(registry);
        types::register(registry);
    }
}
