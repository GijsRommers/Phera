//! `imagerotate($image, 17.5, $background)` samples an inverse rotation.

use phera_extension_api::{Context, NativeResult, Value, args};

use crate::core::{value_error, with, wrap};
use crate::pixels::{Color, Image};

use super::scale::weight;

pub(super) fn rotate(context: &mut dyn Context, values: &[Value]) -> NativeResult {
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
    let angle = angle as f32;
    let hundredths = ((angle * 100.0).floor() as i32).rem_euclid(36000);
    let output = with(values, 0, |image| {
        if hundredths == 0 {
            return Ok(Some(image.clone()));
        }
        let background = if image.is_true_color() || index >= 256 {
            Color(index as u32)
        } else {
            image.color(index).unwrap_or(Color::BLACK)
        };
        let mut converted;
        let source = if image.is_true_color() {
            image
        } else {
            converted = image.clone();
            if !converted.palette_to_true_color() {
                return Ok(None);
            }
            &converted
        };
        Ok(match hundredths {
            9000 | 18000 | 27000 => right_angle(source, hundredths),
            _ => arbitrary(source, angle, background),
        })
    })?;
    output.map_or(Ok(Value::Bool(false)), |image| wrap(context, image))
}

fn right_angle(image: &Image, angle: i32) -> Option<Image> {
    let (width, height) = if angle == 18000 {
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
                _ => (y, i64::from(image.height) - 1 - x),
            };
            output.set(x, y, image.index(image.location(sx, sy)?));
        }
    }
    Some(output)
}

fn arbitrary(image: &Image, angle: f32, background: Color) -> Option<Image> {
    let (sin, cos) = f64::from(angle).to_radians().sin_cos();
    let width = f64::from(image.width);
    let height = f64::from(image.height);
    let rotated_width = (width * cos.abs() + height * sin.abs()).ceil() as u32;
    let rotated_height = (height * cos.abs() + width * sin.abs()).ceil() as u32;
    let mut output = Image::create(rotated_width, rotated_height, true)?;
    output.blend = false;
    output.save_alpha = true;

    // GD 2.3.3 uses 8 fractional bits and integer centers here. Nearest
    // neighbour adds half a pixel before taking the integer coordinate.
    let (sin, cos) = f64::from(angle / 180.0 * std::f32::consts::PI).sin_cos();
    let (sin, cos) = ((sin * 256.0) as i64, (cos * 256.0) as i64);
    let nearest_neighbour = image.interpolation == 16;
    let center_offset = if nearest_neighbour { 128 } else { 0 };
    let sx_center = i64::from(image.width / 2) * 256 + center_offset;
    let sy_center = i64::from(image.height / 2) * 256 + center_offset;
    for y in 0..i64::from(rotated_height) {
        let dy = y - i64::from(rotated_height / 2);
        for x in 0..i64::from(rotated_width) {
            let dx = x - i64::from(rotated_width / 2);
            let sx = dx * cos - dy * sin + sx_center;
            let sy = dx * sin + dy * cos + sy_center;
            let color = if nearest_neighbour {
                nearest(image, sx >> 8, sy >> 8, background)
            } else {
                interpolate(image, sx, sy, background)
            };
            output.set(x, y, color.0 as i32);
        }
    }
    output.blend = true;
    Some(output)
}

fn nearest(image: &Image, x: i64, y: i64, background: Color) -> Color {
    if x <= 0 || y <= 0 || x >= i64::from(image.width) - 1 || y >= i64::from(image.height) - 1 {
        return background;
    }
    image
        .location(x, y)
        .map_or(background, |location| image.true_pixel(location))
}

fn interpolate(image: &Image, x: i64, y: i64, background: Color) -> Color {
    if x < -256 || y < -256 || x >> 8 >= i64::from(image.width) || y >> 8 >= i64::from(image.height)
    {
        return background;
    }
    let (left, top) = (x / 256 - 1, y / 256 - 1);
    let kernel = |coord, first| {
        [0, 1, 2, 3].map(|step| {
            weight(
                image.interpolation,
                ((first + step) * 256 - coord) as f64 / 256.0,
            )
        })
    };
    let horizontal = kernel(x, left);
    let vertical = kernel(y, top);
    let mut channels = [0.0; 4];
    for (row, vertical) in (top..).zip(vertical) {
        for (column, horizontal) in (left..).zip(horizontal) {
            let color = image
                .location(column, row)
                .filter(|&location| image.transparent != Some(image.index(location)))
                .map_or(background, |location| image.true_pixel(location));
            for (total, channel) in channels.iter_mut().zip(color.components()) {
                *total += vertical * horizontal * f64::from(channel);
            }
        }
    }
    let [r, g, b, a] = channels.map(|channel| channel as u8);
    Color::gd(r, g, b, a.min(127))
}
