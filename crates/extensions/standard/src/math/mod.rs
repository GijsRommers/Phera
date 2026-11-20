//! Number functions: rounding, conversions between bases, `min` and
//! `max`, `array_sum`, and the C library's float functions.

use phera_extension_api::{Registry, Value, args};

mod aggregate;
mod base;
mod basic;
mod minmax;
mod misc;
pub(crate) mod rounding;
mod trig;

pub(crate) fn register(registry: &mut Registry) {
    rounding::register_enum(registry);
    minmax::register(registry);
    registry.describe_function("round", "float", None);
    let modules = [
        aggregate::FUNCTIONS,
        base::FUNCTIONS,
        basic::FUNCTIONS,
        misc::FUNCTIONS,
        rounding::FUNCTIONS,
        trig::FUNCTIONS,
    ];
    for entry in modules.into_iter().flatten() {
        registry.function(*entry);
    }
}

/// `function` of a function's one float argument, as a PHP float.
fn of_float(arguments: &[Value], function: impl Fn(f64) -> f64) -> Value {
    Value::Float(function(args::float(arguments, 0).unwrap_or_default()))
}
