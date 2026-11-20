//! Reconstruction kernels used by PHP's `imagescale($image, 32, 32, IMG_MITCHELL)`.

use std::f64::consts::PI;

use crate::pixels::{Color, Image};

pub(super) fn scale(source: &Image, width: u32, height: u32, mode: i32) -> Option<Image> {
    let resized = if width != source.width {
        Some(pass(source, width, source.height, Axis::Horizontal, mode)?)
    } else {
        None
    };
    let input = resized.as_ref().unwrap_or(source);
    if height != source.height {
        pass(input, width, height, Axis::Vertical, mode)
    } else {
        resized
    }
}

#[derive(Clone, Copy)]
enum Axis {
    Horizontal,
    Vertical,
}

fn pass(source: &Image, width: u32, height: u32, axis: Axis, mode: i32) -> Option<Image> {
    let mut output = Image::create(width, height, true)?;
    output.blend = false;
    output.interpolation = mode;
    let (from, to, lines) = match axis {
        Axis::Horizontal => (source.width, width, height),
        Axis::Vertical => (source.height, height, width),
    };
    let support = match mode {
        2 | 17 => 0.0,
        7 | 11 => 0.5,
        1 | 8 | 18 => 1.5,
        10 => 1.25,
        4 | 5 | 9 | 15 => 2.0,
        _ => 1.0,
    };
    let scale = f64::from(to) / f64::from(from);
    let filter_scale = scale.min(1.0);
    let radius = support / filter_scale;
    for target in 0..to {
        let center = f64::from(target) / scale;
        let mut left = ((center - radius).floor() as i64).max(0);
        let right = ((center + radius).ceil() as i64).min(i64::from(from) - 1);
        let window = 2 * radius.ceil() as i64 + 1;
        if right - left + 1 > window {
            left += 1;
        }
        let mut weights: Vec<_> = (left..=right)
            .map(|index| filter_scale * weight(mode, filter_scale * (center - index as f64)))
            .collect();
        let sum = weights.iter().sum::<f64>();
        if sum < 0.0 {
            return None;
        }
        if sum > 0.0 {
            for weight in &mut weights {
                *weight /= sum;
            }
        }
        for line in 0..i64::from(lines) {
            let mut channels = [0.0; 4];
            for (sample, &weight) in (left..=right).zip(&weights) {
                let (x, y) = match axis {
                    Axis::Horizontal => (sample, line),
                    Axis::Vertical => (line, sample),
                };
                let color = source.true_pixel(source.location(x, y)?);
                for (sum, channel) in channels.iter_mut().zip(color.components()) {
                    *sum += weight * f64::from(channel);
                }
            }
            let [r, g, b, a] = channels.map(|value| value.round() as u8);
            let (x, y) = match axis {
                Axis::Horizontal => (i64::from(target), line),
                Axis::Vertical => (line, i64::from(target)),
            };
            output.set(x, y, Color::gd(r, g, b, a.min(127)).0 as i32);
        }
    }
    output.blend = true;
    Some(output)
}

pub(in super::super) fn weight(mode: i32, signed: f64) -> f64 {
    let x = signed.abs();
    match mode {
        1 if x < 0.5 => 0.75 - x * x,
        1 if x < 1.5 => 0.5 * (x - 1.5).powi(2),
        1 => 0.0,
        2 if x == 0.0 => PI / 4.0,
        2 => bessel_one(PI * x) / (2.0 * x),
        4 | 5 => cubic(x, -1.0),
        6 => 0.42 + 0.5 * (PI * x).cos() + 0.08 * (2.0 * PI * x).cos(),
        7 => f64::from(u8::from((-0.5..0.5).contains(&signed))),
        8 if x < 1.0 => (4.0 + x * x * (3.0 * x - 6.0)) / 6.0,
        8 => (2.0 - x).max(0.0).powi(3) / 6.0,
        9 | 11 => cubic(x, -0.5),
        10 => (-2.0 * x * x).exp() * (2.0 / PI).sqrt(),
        12 if x < 1.0 => (2.0 * x - 3.0) * x * x + 1.0,
        12 => 0.0,
        13 if x < 1.0 => 0.92 * (2.0 * x - 3.0) * x * x + 1.0,
        13 => 0.0,
        14 => 0.5 + 0.5 * (PI * x).cos(),
        15 if x < 1.0 => (16.0 + x * x * (21.0 * x - 36.0)) / 18.0,
        15 if x < 2.0 => (32.0 + x * (-60.0 + x * (36.0 - 7.0 * x))) / 18.0,
        15 => 0.0,
        17 => (1.0 - x * x).max(0.0),
        18 if x <= 0.5 => 1.0 - 2.0 * x * x,
        18 if x <= 1.5 => x * x - 2.5 * x + 1.5,
        18 => 0.0,
        19 if x == 0.0 => 1.0,
        19 => (PI * x).sin() / (PI * x),
        _ => (1.0 - x).max(0.0),
    }
}

fn cubic(x: f64, a: f64) -> f64 {
    if x < 1.0 {
        1.0 + x * x * ((a + 2.0) * x - (a + 3.0))
    } else if x < 2.0 {
        a * (x * (x * (x - 5.0) + 8.0) - 4.0)
    } else {
        0.0
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
