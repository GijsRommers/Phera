//! `pack('nC', 258, 3)` writes the bytes `01 02 03`.

mod format;
mod pack;
mod unpack;

use phera_extension_api::{NativeError, Registry};

pub fn register(registry: &mut Registry) {
    registry.function_with_extra_named_handler(pack::PACK, |_, _| {
        Err(NativeError::argument_count_error(
            "pack() does not accept unknown named parameters",
        ))
    });
    registry.function(unpack::UNPACK);
}
