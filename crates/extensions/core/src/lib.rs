//! Functions every script expects: inspecting values (`var_dump`, `print_r`, `var_export`), type tests and conversions, `count`, and the engine's constants.

use phera_extension_api::{Extension, Registry};

pub struct Core;

impl Extension for Core {
    fn name(&self) -> &'static str {
        "core"
    }

    fn register(&self, _registry: &mut Registry) {}
}
