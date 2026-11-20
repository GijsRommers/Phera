//! GD scales via fixed-point nearest or bilinear sampling, or a reconstruction kernel.

mod kernel;

use phera_extension_api::{
    ConstantValue as C, Context, FunctionEntry as F, NativeResult, Param as P, ParamType as T,
    Registry, Value, args,
};

use crate::core::{IMAGE, value_error, with, with_mut, wrap};
use crate::pixels::{Color, Image};

const BILINEAR: C = C::Int(3);
const METHOD: P = P::new("method", T::Int).with_constant_default("IMG_BILINEAR_FIXED", BILINEAR);

pub(super) fn register(registry: &mut Registry) {
    for (entry, result) in const {
        [
            (
                F::sync(
                    "imagescale",
                    const {
                        &[
                            IMAGE,
                            P::new("width", T::Int),
                            P::new("height", T::Int).with_default(C::Int(-1)),
                            P::new("mode", T::Int)
                                .with_constant_default("IMG_BILINEAR_FIXED", BILINEAR),
                        ]
                    },
                    scale,
                ),
                "GdImage|false",
            ),
            (
                F::sync("imagesetinterpolation", &[IMAGE, METHOD], set_interpolation),
                "bool",
            ),
            (
                F::sync("imagegetinterpolation", &[IMAGE], |_, args| {
                    with(args, 0, |image| {
                        Ok(Value::Int(i64::from(image.interpolation)))
                    })
                }),
                "int",
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

fn set_interpolation(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let method = args::int(values, 1).unwrap_or(3);
    if !(0..=22).contains(&method) {
        return Err(value_error(
            "imagesetinterpolation",
            2,
            "method",
            "must be one of the IMG_* constants",
        ));
    }
    with_mut(values, 0, |image| {
        image.interpolation = if method == 0 { 22 } else { method as i32 };
        Ok(Value::Bool(true))
    })
}

fn scale(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mut width = args::int(values, 1).unwrap_or(0);
    let mut height = args::int(values, 2).unwrap_or(-1);
    let mode = args::int(values, 3).unwrap_or(3);
    if !(0..22).contains(&mode) {
        return Err(value_error(
            "imagescale",
            4,
            "mode",
            "must be one of the GD_* constants",
        ));
    }
    if width < 0 && height < 0 {
        return Err(phera_extension_api::NativeError::value_error(
            "Argument #2 ($width) and argument #3 ($height) cannot be both negative",
        ));
    }
    let output = with(values, 0, |image| {
        if width < 0 {
            width = proportional(height, image.width, image.height, 3, "height")?;
        }
        if height < 0 {
            height = proportional(width, image.height, image.width, 2, "width")?;
        }
        for (value, index, name) in [(width, 2, "width"), (height, 3, "height")] {
            if !(1..=i64::from(i32::MAX)).contains(&value) {
                return Err(value_error(
                    "imagescale",
                    index,
                    name,
                    "must be between 1 and 2147483647",
                ));
            }
        }
        if width == i64::from(image.width) && height == i64::from(image.height) {
            let mut image = image.clone();
            image.transparent = None;
            return Ok(Some(image));
        }
        if mode == 21 {
            return Ok(None);
        }
        Ok(scale_pixels(
            image,
            width as u32,
            height as u32,
            mode as i32,
        ))
    })?;
    output.map_or(Ok(Value::Bool(false)), |image| wrap(context, image))
}

fn proportional(
    value: i64,
    numerator: u32,
    denominator: u32,
    index: usize,
    parameter: &str,
) -> Result<i64, phera_extension_api::NativeError> {
    let maximum = i64::MAX / i64::from(numerator);
    value.checked_mul(i64::from(numerator)).map_or_else(
        || {
            Err(value_error(
                "imagescale",
                index,
                parameter,
                &format!("must be less than or equal to {maximum}"),
            ))
        },
        |product| Ok(product / i64::from(denominator)),
    )
}

fn scale_pixels(image: &Image, width: u32, height: u32, mode: i32) -> Option<Image> {
    if !matches!(mode, 0 | 3 | 16) {
        return kernel::scale(image, width, height, mode);
    }
    let mut output = Image::create(width, height, true)?;
    output.blend = false;
    let dx = (image.width as f32 / width as f32 * 256.0) as i64;
    let dy = (image.height as f32 / height as f32 * 256.0) as i64;
    for y in 0..i64::from(height) {
        for x in 0..i64::from(width) {
            let color = match mode {
                16 => image
                    .location((x * dx) >> 8, (y * dy) >> 8)
                    .map_or(Color::BLACK, |pos| image.true_pixel(pos)),
                _ => bilinear(image, x * dx, y * dy, None),
            };
            output.set(x, y, color.0 as i32);
        }
    }
    output.blend = true;
    Some(output)
}

pub(super) fn bilinear(image: &Image, x: i64, y: i64, background: Option<Color>) -> Color {
    let (left, top) = (x >> 8, y >> 8);
    let (fx, fy) = (x & 255, y & 255);
    let weights = [
        (256 - fy) * (256 - fx),
        (256 - fy) * fx,
        fy * (256 - fx),
        fy * fx,
    ];
    let pixel = |x, y, fallback| {
        image
            .location(x, y)
            .filter(|&position| image.transparent != Some(image.index(position)))
            .map_or(fallback, |index| image.true_pixel(index))
    };
    let first = pixel(left, top, background.unwrap_or(Color::BLACK));
    let fallback = background.unwrap_or(first);
    let colors = [
        first,
        pixel(left + 1, top, fallback),
        pixel(left, top + 1, fallback),
        pixel(left + 1, top + 1, fallback),
    ];
    let mut result = [0i64; 4];
    for (color, weight) in colors.into_iter().zip(weights) {
        for (output, channel) in result.iter_mut().zip(color.components()) {
            *output += i64::from(channel) * (weight >> 8);
        }
    }
    let [r, g, b, a] = result.map(|value| (value >> 8) as u8);
    Color::gd(r, g, b, a)
}
