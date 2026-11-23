//! A 3x3 `imageconvolution` preserves GD alpha and clamps the sampled edges.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry as F, NativeError, NativeResult, Param as P, ParamType as T,
    Registry, Value, args, value::to_float,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::{Color, Image};

use super::store_color;

pub(super) fn register(registry: &mut Registry) {
    let entry = F::sync(
        "imageconvolution",
        const {
            &[
                IMAGE,
                P::new("matrix", T::Array),
                P::new("divisor", T::Float),
                P::new("offset", T::Float),
            ]
        },
        convolution,
    );
    registry.function(entry);
    registry.describe_function(entry.name, "bool", None);
}

fn convolution(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    const NAME: &str = "imageconvolution";
    let matrix = args::array(values, 1).ok_or_else(|| NativeError::error("Missing matrix"))?;
    if matrix.len() != 3 {
        return Err(value_error(NAME, 2, "matrix", "must be a 3x3 array"));
    }
    let mut kernel = [[0f32; 3]; 3];
    for (i, coeff) in kernel.iter_mut().enumerate() {
        let Some(Value::Array(row)) = matrix.get(&ArrayKey::Int(i as i64)) else {
            continue;
        };
        if row.len() != 3 {
            return Err(value_error(
                NAME,
                2,
                "matrix",
                &format!(
                    "must be a 3x3 array, matrix[{i}] only has {} elements",
                    row.len()
                ),
            ));
        }
        for (j, weight) in coeff.iter_mut().enumerate() {
            let value = row.get(&ArrayKey::Int(j as i64)).ok_or_else(|| value_error(
                NAME, 2, "matrix",
                &format!("must be a 3x3 array, matrix[{i}][{j}] cannot be found (missing integer key)"),
            ))?;
            *weight = to_float(value, context.diagnostics())? as f32;
        }
    }
    let divisor = args::float(values, 2).unwrap_or_default();
    let offset = args::float(values, 3).unwrap_or_default();
    if !divisor.is_finite() {
        return Err(value_error(NAME, 3, "divisor", "must be finite"));
    }
    let divisor = divisor as f32;
    if divisor == 0.0 {
        return Err(value_error(NAME, 3, "divisor", "must not be 0"));
    }
    if !offset.is_finite() {
        return Err(value_error(NAME, 4, "offset", "must be finite"));
    }
    with_mut(values, 0, |image| {
        Ok(Value::Bool(apply(image, kernel, divisor, offset as f32)))
    })
}

pub(super) fn apply(image: &mut Image, matrix: [[f32; 3]; 3], divisor: f32, offset: f32) -> bool {
    let input = image.clone();
    let width = i64::from(input.width);
    let height = i64::from(input.height);
    for y in 0..height {
        for x in 0..width {
            let position = (y * width + x) as usize;
            let mut outputs = [0.0; 3];
            for (row, coefficients) in matrix.into_iter().enumerate() {
                let source_y = (y - 1 + row as i64).clamp(0, height - 1);
                for (column, weight) in coefficients.into_iter().enumerate() {
                    let source_x = (x - 1 + column as i64).clamp(0, width - 1);
                    let offset = (source_y * width + source_x) as usize;
                    for (sum, component) in outputs
                        .iter_mut()
                        .zip(input.true_pixel(offset).components())
                    {
                        *sum += weight * f32::from(component);
                    }
                }
            }
            let [r, g, b] = outputs.map(|value| (value / divisor + offset) as u8);
            let alpha = input.true_pixel(position).components()[3];
            store_color(image, position, Color::gd(r, g, b, alpha));
        }
    }
    true
}

pub(super) fn selective(image: &mut Image) -> bool {
    let width = i64::from(image.width);
    let height = i64::from(image.height);
    for y in 0..height {
        for x in 0..width {
            let position = (y * width + x) as usize;
            let center = image.true_pixel(position).components();
            let mut cells = [([0; 4], [0.0f32; 3]); 9];
            let mut sums = [0.0; 3];
            for (i, (pixel, weights)) in cells.iter_mut().enumerate() {
                let (dx, dy) = ((i % 3) as i64 - 1, (i / 3) as i64 - 1);
                *pixel = image
                    .location(x + dx, y + dy)
                    .map_or([0; 4], |p| image.true_pixel(p).components());
                for (channel, weight) in weights.iter_mut().enumerate() {
                    let difference = f32::from(center[channel].abs_diff(pixel[channel]));
                    *weight = if i == 4 {
                        0.5
                    } else {
                        difference.max(1.0).recip()
                    };
                    sums[channel] += *weight;
                }
            }
            let mut output = [0.0; 3];
            for (pixel, weights) in cells {
                for (channel, weight) in weights.into_iter().enumerate() {
                    output[channel] += f32::from(pixel[channel]) * (weight / sums[channel]);
                }
            }
            let [r, g, b] = output.map(|value| value as u8);
            store_color(image, position, Color::gd(r, g, b, cells[8].0[3]));
        }
    }
    true
}
