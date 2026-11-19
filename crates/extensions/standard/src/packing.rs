//! `pack('nC', 258, 3)` writes the bytes `01 02 03`.

mod format;
mod pack;
mod unpack;

use phera_extension_api::{Change, NativeError, Registry};

pub fn register(registry: &mut Registry) {
    let level = registry.level();
    let (pack, unpack) = if level.has(Change::PackEndiannessModifiers) {
        (pack::entry::<true>(), unpack::entry::<true>())
    } else {
        (pack::entry::<false>(), unpack::entry::<false>())
    };
    let extra_named = if level.has(Change::PackNamedVariadicsError) {
        |_: &mut dyn phera_extension_api::Context, _: &[phera_extension_api::Value]| {
            Err(NativeError::argument_count_error(
                "Internal function pack() does not accept named variadic arguments",
            ))
        }
    } else {
        |_: &mut dyn phera_extension_api::Context, _: &[phera_extension_api::Value]| {
            Err(NativeError::argument_count_error(
                "pack() does not accept unknown named parameters",
            ))
        }
    };
    registry.function_with_extra_named_handler(pack, extra_named);
    registry.function(unpack);
}
