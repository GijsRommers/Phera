//! Draw text using one of PHP's five built-in GD fonts, as in `imagestring($i, 5, 0, 0, "P", 0)`.

mod bitmap;
mod outline;

use phera_extension_api::Registry;

pub fn register(registry: &mut Registry) {
    bitmap::register(registry);
    outline::register(registry);
}
