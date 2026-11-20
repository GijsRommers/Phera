//! Image geometry, such as `imagecopyresampled($thumb, $source, 0, 0, 0, 0, 16, 16, 64, 64)`.

mod copy;
mod transform;

use phera_extension_api::Registry;

pub fn register(registry: &mut Registry) {
    copy::register(registry);
    transform::register(registry);
}
