//! `imagecrop($image, ['x' => 1, 'y' => 1, 'width' => 3, 'height' => 2])`.

use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, NativeError, NativeResult,
    Param as P, ParamType as T, Registry, Value, args, value::to_int,
};

use crate::core::{IMAGE, value_error, with, wrap};
use crate::geometry::copy::{Region, unscaled};
use crate::pixels::{Color, Image};

pub(super) fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::sync(
                "imagecrop",
                const { &[IMAGE, P::new("rectangle", T::Array)] },
                crop,
            ),
            F::sync(
                "imagecropauto",
                const {
                    &[
                        IMAGE,
                        P::new("mode", T::Int).with_constant_default("IMG_CROP_DEFAULT", C::Int(0)),
                        P::new("threshold", T::Float).with_default(C::Float(0.5)),
                        P::new("color", T::Int).with_default(C::Int(-1)),
                    ]
                },
                crop_auto,
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "GdImage|false", None);
    }
}

fn crop(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let rect = args::array(values, 1).ok_or_else(|| NativeError::error("Missing rectangle"))?;
    let mut keys = [0i32; 4];
    for (slot, key) in keys.iter_mut().zip(["x", "y", "width", "height"]) {
        let value = rect.get(&ArrayKey::str(key)).ok_or_else(|| {
            let article = if key == "x" { "an" } else { "a" };
            value_error(
                "imagecrop",
                2,
                "rectangle",
                &format!("must have {article} \"{key}\" key"),
            )
        })?;
        *slot = to_int(value, context.diagnostics())? as i32;
    }
    let [x, y, width, height] = keys;
    for (first, size, name, dimension) in [(x, width, "x", "width"), (y, height, "y", "height")] {
        if first.checked_add(size).is_none() {
            let error = if size > 0 { "overflow" } else { "underflow" };
            return Err(value_error(
                "imagecrop",
                2,
                "rectangle",
                &format!("{error} with \"{name}\" and \"{dimension}\" keys"),
            ));
        }
    }
    let region = Region {
        x: x.into(),
        y: y.into(),
        width: width.into(),
        height: height.into(),
    };
    let image = with(values, 0, |image| Ok(extract(image, region)))?;
    image.map_or(Ok(Value::Bool(false)), |image| wrap(context, image))
}

fn extract(image: &Image, source: Region) -> Option<Image> {
    let width = u32::try_from(source.width).ok()?;
    let height = u32::try_from(source.height).ok()?;
    let mut output = Image::create(width, height, image.is_true_color())?;
    let was_blending = std::mem::replace(&mut output.blend, false);
    unscaled(
        &mut output,
        image,
        Region {
            x: 0,
            y: 0,
            ..source
        },
        source,
    );
    output.blend = was_blending;
    Some(output)
}

fn crop_auto(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mode = args::int(values, 1).unwrap_or(0);
    let threshold = f64::from(args::float(values, 2).unwrap_or(0.5) as f32);
    let index = args::int(values, 3).unwrap_or(-1);
    let image = with(values, 0, |image| {
        let index = match mode {
            0 | 1 => image.transparent.unwrap_or(-1),
            2 => image.closest(Color::BLACK),
            3 => image.closest(Color::gd(255, 255, 255, 0)),
            4 => from_corners(image),
            5 => {
                if index < 0 || (!image.is_true_color() && image.color(index as i32).is_none()) {
                    return Err(value_error(
                        "imagecropauto",
                        4,
                        "color",
                        "must be greater than or equal to 0 when using the threshold mode",
                    ));
                }
                if threshold > 100.0 {
                    return Ok(None);
                }
                index as i32
            }
            _ => {
                return Err(value_error(
                    "imagecropauto",
                    2,
                    "mode",
                    "must be a valid mode",
                ));
            }
        };
        let matches = |position| {
            let candidate = image.index(position);
            if mode != 5 {
                return candidate == index;
            }
            let a = image.color(index).unwrap_or(Color::BLACK);
            let b = image.true_pixel(position);
            let distance: i32 = a
                .components()
                .into_iter()
                .zip(b.components())
                .map(|(a, b)| {
                    let difference = i32::from(a) - i32::from(b);
                    difference * difference
                })
                .sum();
            100.0 * f64::from(distance) / 195_075.0 < threshold
        };
        Ok(bounds(image, matches).and_then(|region| extract(image, region)))
    })?;
    image.map_or(Ok(Value::Bool(false)), |image| wrap(context, image))
}

fn bounds(image: &Image, matches: impl Fn(usize) -> bool) -> Option<Region> {
    let width = image.width as usize;
    let height = image.height as usize;
    let row_matches = |y| (0..width).all(|x| matches(y * width + x));
    let top = (0..height).find(|&y| !row_matches(y))?;
    let bottom = (top..height).rfind(|&y| !row_matches(y))? + 1;
    let col_matches = |x| (0..bottom).all(|y| matches(y * width + x));
    let left = (0..width).find(|&x| !col_matches(x))?;
    let right = (left..width).rfind(|&x| !col_matches(x))? + 1;
    Some(Region {
        x: left as i64,
        y: top as i64,
        width: (right - left) as i64,
        height: (bottom - top) as i64,
    })
}

fn from_corners(image: &Image) -> i32 {
    let width = image.width as usize;
    let height = image.height as usize;
    let corners = [0, width - 1, (height - 1) * width, height * width - 1].map(|i| image.index(i));
    for copies in [3, 2] {
        if let Some(index) = corners
            .iter()
            .copied()
            .find(|color| corners.iter().filter(|&other| other == color).count() >= copies)
        {
            return index;
        }
    }
    let mut totals = [0u16; 4];
    for corner in corners {
        let color = image.color(corner).unwrap_or(Color::BLACK);
        for (total, channel) in totals.iter_mut().zip(color.components()) {
            *total += u16::from(channel);
        }
    }
    let [r, g, b, a] = totals.map(|value| (value / 4) as u8);
    image.closest(Color::gd(r, g, b, a))
}
