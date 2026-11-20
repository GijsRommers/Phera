//! PHP can crop, resize, rotate, or flip an existing GdImage.

mod crop;
mod rotation;
mod scale;

use phera_extension_api::{
    Context, FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value, args,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::Storage;

pub(super) fn register(registry: &mut Registry) {
    crop::register(registry);
    scale::register(registry);
    for (entry, result) in const {
        [
            (
                F::sync(
                    "imageflip",
                    const { &[IMAGE, P::new("mode", T::Int)] },
                    flip,
                ),
                "true",
            ),
            (
                F::sync(
                    "imagerotate",
                    const {
                        &[
                            IMAGE,
                            P::new("angle", T::Float),
                            P::new("background_color", T::Int),
                        ]
                    },
                    rotation::rotate,
                ),
                "GdImage|false",
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

fn flip(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mode = args::int(values, 1).unwrap_or_default();
    if !(1..=3).contains(&mode) {
        return Err(value_error(
            "imageflip",
            2,
            "mode",
            "must be one of IMG_FLIP_VERTICAL, IMG_FLIP_HORIZONTAL, or IMG_FLIP_BOTH",
        ));
    }
    with_mut(values, 0, |image| {
        match &mut image.storage {
            Storage::TrueColor(pixels) => flip_pixels(pixels, image.width as usize, mode),
            Storage::Indexed { pixels, .. } => flip_pixels(pixels, image.width as usize, mode),
        }
        Ok(Value::Bool(true))
    })
}

fn flip_pixels<T>(pixels: &mut [T], width: usize, mode: i64) {
    match mode {
        1 => pixels.chunks_exact_mut(width).for_each(<[T]>::reverse),
        2 => {
            pixels.reverse();
            pixels.chunks_exact_mut(width).for_each(<[T]>::reverse);
        }
        _ => pixels.reverse(),
    }
}
