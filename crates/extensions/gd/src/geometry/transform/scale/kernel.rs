//! Reconstruction kernels used by PHP's `imagescale($image, 32, 32, IMG_MITCHELL)`.

use std::f64::consts::PI;

use crate::pixels::{Color, Image};

pub(super) fn sample(image: &Image, x: i64, y: i64, width: u32, height: u32, mode: i32) -> Color {
    let sx = f64::from(image.width) / f64::from(width);
    let sy = f64::from(image.height) / f64::from(height);
    let cx = (x as f64 + 0.5) * sx - 0.5;
    let cy = (y as f64 + 0.5) * sy - 0.5;
    let radius = match mode {
        7 => 0.5,
        1 | 18 => 1.5,
        4 | 5 | 8 | 9 | 11 | 15 => 2.0,
        _ => 1.0,
    };
    let sx = sx.max(1.0);
    let sy = sy.max(1.0);
    let x_range = window(cx, sx * radius, image.width);
    let y_range = window(cy, sy * radius, image.height);
    let mut sums = [0.0; 4];
    let mut weight = 0.0;
    for row in y_range {
        let wy = kernel(mode, (row as f64 - cy) / sy);
        for column in x_range.clone() {
            let wx = kernel(mode, (column as f64 - cx) / sx);
            let coefficient = wx * wy;
            let Some(pos) = image.location(column, row) else {
                continue;
            };
            for (sum, channel) in sums.iter_mut().zip(image.true_pixel(pos).components()) {
                *sum += coefficient * f64::from(channel);
            }
            weight += coefficient;
        }
    }
    if weight.abs() < f64::EPSILON {
        let pos = image.location(cx.round() as i64, cy.round() as i64);
        return pos.map_or(Color::BLACK, |pos| image.true_pixel(pos));
    }
    let [r, g, b, a] = sums.map(|channel| (channel / weight).round() as u8);
    Color::gd(r, g, b, a.min(127))
}

fn window(center: f64, radius: f64, dimension: u32) -> std::ops::RangeInclusive<i64> {
    let lower = ((center - radius).ceil() as i64).max(0);
    let upper = ((center + radius).floor() as i64).min(i64::from(dimension) - 1);
    lower..=upper
}

fn kernel(mode: i32, signed: f64) -> f64 {
    let x = signed.abs();
    match mode {
        1 if x < 0.5 => 0.75 - x * x,
        1 => 0.5 * (x - 1.5).powi(2),
        2 if x == 0.0 => PI / 4.0,
        2 => bessel_one(PI * x) / (2.0 * x),
        4 | 5 => cubic(x, -1.0),
        6 => 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos(),
        7 => f64::from(u8::from((-0.5..0.5).contains(&signed))),
        8 if x < 1.0 => (4.0 + x * x * (3.0 * x - 6.0)) / 6.0,
        8 => (2.0 - x).powi(3) / 6.0,
        9 | 11 => cubic(x, -0.5),
        10 => (-2.0 * x * x).exp() * (2.0 / PI).sqrt(),
        12 => (2.0 * x - 3.0) * x * x + 1.0,
        13 => 0.92 * (2.0 * x - 3.0) * x * x + 1.0,
        14 => 0.5 + 0.5 * (PI * x).cos(),
        15 if x < 1.0 => (16.0 + x * x * (21.0 * x - 36.0)) / 18.0,
        15 => (32.0 + x * (-60.0 + x * (36.0 - 7.0 * x))) / 18.0,
        17 => 1.0 - x * x,
        18 if x <= 0.5 => 1.0 - 2.0 * x * x,
        18 => x * x - 2.5 * x + 1.5,
        19 if x == 0.0 => 1.0,
        19 => (PI * x).sin() / (PI * x),
        _ => (1.0 - x).max(0.0),
    }
}

fn cubic(x: f64, a: f64) -> f64 {
    if x < 1.0 {
        1.0 + x * x * ((a + 2.0) * x - (a + 3.0))
    } else {
        a * (x * (x * (x - 5.0) + 8.0) - 4.0)
    }
}

fn bessel_one(x: f64) -> f64 {
    let half = x / 2.0;
    let mut term = half;
    let mut sum = term;
    for index in 1..12 {
        term *= -half * half / f64::from(index * (index + 1));
        sum += term;
    }
    sum
}
