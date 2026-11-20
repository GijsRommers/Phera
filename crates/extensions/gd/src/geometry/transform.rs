//! PHP can crop, resize, rotate, or flip an existing GdImage.

mod crop;
mod scale;

use phera_extension_api::{
    Context, FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value, args,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::{Color, Image, Storage};

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
                    rotate,
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

fn rotate(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let angle = args::float(values, 1).unwrap_or_default();
    if !(-21_474_836.0..=21_474_836.0).contains(&angle) {
        return Err(value_error(
            "imagerotate",
            2,
            "angle",
            "must be between -21474836 and 21474836",
        ));
    }
    let index = args::int(values, 2).unwrap_or_default() as i32;
    if index < 0 {
        return Ok(Value::Bool(false));
    }
    let output = with_mut(values, 0, |image| {
        let background = image.color(index).unwrap_or(Color::BLACK);
        if !image.palette_to_true_color() {
            return Ok(None);
        }
        let angle_fixed = (((angle as f32) * 100.0).floor() as i32).rem_euclid(36000);
        let out = match angle_fixed {
            0 | 9000 | 18000 | 27000 => rotate_right_angle(image, angle_fixed),
            _ => rotate_arbitrary(image, angle, background),
        };
        Ok(out)
    })?;
    output.map_or(Ok(Value::Bool(false)), |image| {
        crate::core::wrap(context, image)
    })
}

fn rotate_right_angle(image: &Image, angle: i32) -> Option<Image> {
    let (width, height) = if angle % 18000 == 0 {
        (image.width, image.height)
    } else {
        (image.height, image.width)
    };
    let mut output = Image::create(width, height, true)?;
    output.blend = false;
    output.save_alpha = true;
    output.transparent = image.transparent;
    for y in 0..i64::from(height) {
        for x in 0..i64::from(width) {
            let (sx, sy) = match angle {
                9000 => (i64::from(image.width) - 1 - y, x),
                18000 => (
                    i64::from(image.width) - 1 - x,
                    i64::from(image.height) - 1 - y,
                ),
                27000 => (y, i64::from(image.height) - 1 - x),
                _ => (x, y),
            };
            let position = image.location(sx, sy)?;
            let index = image.index(position);
            if angle != 0 || image.transparent != Some(index) {
                output.set(x, y, index);
            }
        }
    }
    Some(output)
}

fn rotate_arbitrary(image: &Image, angle: f64, background: Color) -> Option<Image> {
    let (sin, cos) = angle.to_radians().sin_cos();
    let width = f64::from(image.width);
    let height = f64::from(image.height);
    let rotated_width = (width * cos.abs() + height * sin.abs()).ceil() as u32;
    let rotated_height = (height * cos.abs() + width * sin.abs()).ceil() as u32;
    let mut output = Image::create(rotated_width, rotated_height, true)?;
    output.blend = false;
    output.save_alpha = true;
    let source_center = ((width - 1.0) / 2.0, (height - 1.0) / 2.0);
    let center = (
        (f64::from(rotated_width) - 1.0) / 2.0,
        (f64::from(rotated_height) - 1.0) / 2.0,
    );
    for y in 0..i64::from(rotated_height) {
        for x in 0..i64::from(rotated_width) {
            let dx = x as f64 - center.0;
            let dy = y as f64 - center.1;
            let sx = dx * cos - dy * sin + source_center.0;
            let sy = dx * sin + dy * cos + source_center.1;
            let color = if image.interpolation == 16 {
                image
                    .location(sx.round() as i64, sy.round() as i64)
                    .map_or(background, |pos| image.true_pixel(pos))
            } else {
                scale::bilinear(
                    image,
                    (sx * 256.0) as i64,
                    (sy * 256.0) as i64,
                    Some(background),
                )
            };
            output.set(x, y, color.0 as i32);
        }
    }
    Some(output)
}
