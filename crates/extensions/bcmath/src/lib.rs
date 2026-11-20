//! PHP BCMath support. `bcdiv('1', '8', 4)` returns `'0.1250'`.

mod arithmetic;
mod decimal;
mod functions;
mod number;

use phera_extension_api::{Extension, Registry};

pub struct BcMath;

impl Extension for BcMath {
    fn name(&self) -> &'static str {
        "bcmath"
    }

    fn register(&self, registry: &mut Registry) {
        for (function, return_type) in functions::FUNCTIONS {
            registry.function(*function);
            registry.describe_function(function.name, return_type, None);
        }
        registry.class(number::CLASS);
        registry.ini_update("bcmath.scale", functions::update_scale);
    }
}
