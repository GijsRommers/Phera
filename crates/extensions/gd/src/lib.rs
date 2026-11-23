//! `imagecreatetruecolor(32, 16)` creates a request-local GD image.

mod codec;
mod color;
mod constants;
mod core;
mod drawing;
mod filters;
mod geometry;
mod metadata;
mod pixels;
mod quantize;
mod text;

pub use metadata::{ExifImageType, ImageMetadata};
use phera_extension_api::{Extension, Registry};

pub struct Gd;

impl Extension for Gd {
    fn name(&self) -> &'static str {
        "gd"
    }

    fn register(&self, registry: &mut Registry) {
        constants::register(registry);
        core::register(registry);
        codec::register(registry);
        quantize::register(registry);
        color::register(registry);
        drawing::register(registry);
        geometry::register(registry);
        filters::register(registry);
        text::register(registry);
    }
}
