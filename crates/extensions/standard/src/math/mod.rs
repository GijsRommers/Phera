//! Registration of PHP arithmetic, rounding, radix, and array aggregation functions.

use phera_extension_api::Registry;

mod aggregate;
mod base;
mod basic;
mod minmax;
mod misc;
pub(crate) mod rounding;
mod trig;

pub(crate) fn register(registry: &mut Registry) {
    for entry in rounding::FUNCTIONS {
        registry.function(*entry);
    }
    base::register(registry);
    aggregate::register(registry);
    basic::register(registry);
    minmax::register(registry);
    misc::register(registry);
    trig::register(registry);
}
