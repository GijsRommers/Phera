//! GD's `IMG_FILTER_PIXELATE` and `IMG_FILTER_SCATTER` move or combine pixels.

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;

use phera_extension_api::{Context, NativeResult, Value, args, value::to_int};

use crate::core::value_error;
use crate::pixels::{Color, Image};

pub(super) fn pixelate(image: &mut Image, block: i32, average: bool) -> bool {
    if block <= 0 {
        return false;
    }
    if block == 1 {
        return true;
    }
    let step = block as usize;
    let width = image.width as usize;
    let height = image.height as usize;
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            let start = y * width + x;
            let max_y = (y + step).min(height);
            let max_x = (x + step).min(width);
            let color = if average {
                let mut sum = [0u64; 4];
                for source_y in y..max_y {
                    for source_x in x..max_x {
                        for (output, input) in sum
                            .iter_mut()
                            .zip(image.true_pixel(source_y * width + source_x).components())
                        {
                            *output += u64::from(input);
                        }
                    }
                }
                let count = ((max_x - x) * (max_y - y)) as u64;
                let [r, g, b, a] = sum.map(|value| (value / count) as u8);
                image.resolve(Color::gd(r, g, b, a))
            } else {
                image.index(start)
            };
            for target_y in y..max_y {
                for target_x in x..max_x {
                    image.set_at(target_y * width + target_x, color);
                }
            }
        }
    }
    true
}

pub(super) fn scatter(
    context: &mut dyn Context,
    image: &mut Image,
    values: &[Value],
) -> NativeResult {
    let read_bound = |index| {
        let bound = args::int(values, index).unwrap_or_default();
        if !(0..=i64::from(i32::MAX)).contains(&bound) {
            return Err(value_error(
                "imagefilter",
                index + 3,
                "args",
                "must be between 0 and 2147483647",
            ));
        }
        Ok(bound)
    };
    let min = read_bound(0)?;
    let max = read_bound(1)?;
    if min == 0 && max == 0 {
        return Ok(Value::Bool(true));
    }
    if min >= max {
        return Ok(Value::Bool(false));
    }
    let colors = args::array(values, 2)
        .map(|filter| {
            filter
                .iter()
                .map(|(_, value)| to_int(value, context.diagnostics()).map(|value| value as i32))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let rng = RandomState::new();
    let span = (max - min) as u64;
    for y in 0..i64::from(image.height) {
        for x in 0..i64::from(image.width) {
            let dx = x + min + (rng.hash_one((x, y, 0)) % span) as i64;
            let dy = y + min + (rng.hash_one((x, y, 1)) % span) as i64;
            let Some(target) = image.location(dx, dy) else {
                continue;
            };
            let Some(source) = image.location(x, y) else {
                continue;
            };
            let pixel = image.index(source);
            if colors.is_empty() || colors.contains(&pixel) {
                let other = image.index(target);
                image.set_at(target, pixel);
                image.set_at(source, other);
            }
        }
    }
    Ok(Value::Bool(true))
}
