//! `imagetruecolortopalette($image, true, 128)` builds a bounded indexed color table.

use std::collections::{HashMap, hash_map::Entry};

use phera_extension_api::{
    Context, FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value, args,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::{Color, Image, Storage};

pub fn register(registry: &mut Registry) {
    let entry = F::sync(
        "imagetruecolortopalette",
        const {
            &[
                IMAGE,
                P::new("dither", T::Bool),
                P::new("num_colors", T::Int),
            ]
        },
        convert,
    );
    registry.function(entry);
    registry.describe_function(entry.name, "bool", None);
}

fn convert(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let count = args::int(values, 2).unwrap_or_default();
    if !(1..=i64::from(i32::MAX)).contains(&count) {
        return Err(value_error(
            "imagetruecolortopalette",
            3,
            "num_colors",
            "must be between 1 and 2147483647",
        ));
    }
    let dither = args::bool(values, 1).unwrap_or(false);
    with_mut(values, 0, |image| {
        reduce(image, dither, (count as usize).min(256));
        Ok(Value::Bool(true))
    })
}

fn reduce(image: &mut Image, dither: bool, count: usize) {
    let Storage::TrueColor(pixels) = &image.storage else {
        return;
    };
    let Some((indexed, palette)) = exact(pixels, count) else {
        let palette = approximate_palette(pixels, count);
        let indexed = assign(pixels, &palette, image.width as usize, dither);
        replace(image, indexed, palette);
        return;
    };
    replace(image, indexed, palette);
}

fn replace(image: &mut Image, pixels: Vec<u8>, palette: Vec<Color>) {
    image.transparent = image
        .transparent
        .map(|color| i32::from(closest(&palette, Color(color as u32)).unwrap_or(0)));
    image.storage = Storage::Indexed { pixels, palette };
    image.blend = false;
}

fn exact(pixels: &[Color], count: usize) -> Option<(Vec<u8>, Vec<Color>)> {
    let mut palette = Vec::new();
    let mut indices = HashMap::new();
    for pixel in pixels {
        if let Entry::Vacant(entry) = indices.entry(pixel.0) {
            if palette.len() == count {
                return None;
            }
            entry.insert(palette.len() as u8);
            palette.push(*pixel);
        }
    }
    let pixels = pixels.iter().map(|pixel| indices[&pixel.0]).collect();
    Some((pixels, palette))
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    sum: [u64; 4],
    count: u64,
}

impl Bucket {
    fn color(self) -> Color {
        let [r, g, b, a] = self.sum.map(|sum| (sum / self.count.max(1)) as u8);
        Color::gd(r, g, b, a)
    }
}

fn assign(pixels: &[Color], palette: &[Color], width: usize, dither: bool) -> Vec<u8> {
    let mut indices = vec![None; 32_768];
    let mut current = vec![[0i32; 3]; width];
    let mut next = vec![[0i32; 3]; width];
    let mut result = Vec::with_capacity(pixels.len());
    for (i, pixel) in pixels.iter().enumerate() {
        let [r, g, b, a] = pixel.components();
        let x = i % width;
        let channels = [r, g, b];
        let corrected = if dither {
            std::array::from_fn(|i| {
                (i32::from(channels[i]) + current[x][i] / 16).clamp(0, 255) as u8
            })
        } else {
            channels
        };
        let [red, green, blue] = corrected;
        let mapped = Color::gd(red, green, blue, a);
        let bin = bucket(mapped.components());
        let index = *indices[bin].get_or_insert_with(|| closest(palette, mapped).unwrap_or(0));
        result.push(index);
        if dither {
            let target = palette[usize::from(index)].components();
            for (channel, value) in corrected.into_iter().enumerate() {
                let error = i32::from(value) - i32::from(target[channel]);
                if x + 1 < width {
                    current[x + 1][channel] += error * 7;
                    next[x + 1][channel] += error;
                }
                if x != 0 {
                    next[x - 1][channel] += error * 3;
                }
                next[x][channel] += error * 5;
            }
            if x + 1 == width {
                std::mem::swap(&mut current, &mut next);
                next.fill([0; 3]);
            }
        }
    }
    result
}

fn bucket([r, g, b, _]: [u8; 4]) -> usize {
    (usize::from(r >> 3) << 10) | (usize::from(g >> 3) << 5) | usize::from(b >> 3)
}

fn approximate_palette(pixels: &[Color], count: usize) -> Vec<Color> {
    let mut histogram = vec![Bucket::default(); 32_768];
    for pixel in pixels {
        let components = pixel.components();
        let bin = &mut histogram[bucket(components)];
        bin.count += 1;
        for (out, value) in bin.sum.iter_mut().zip(components) {
            *out += u64::from(value);
        }
    }
    histogram.retain(|bin| bin.count != 0);
    let mut groups = vec![histogram];
    while groups.len() < count {
        let Some((index, (channel, _))) = groups
            .iter()
            .enumerate()
            .filter(|(_, bins)| bins.len() > 1)
            .map(|(index, bins)| (index, widest(bins)))
            .max_by_key(|(_, (_, width))| *width)
        else {
            break;
        };
        let group = &mut groups[index];
        group.sort_unstable_by_key(|bin| bin.color().components()[channel]);
        let half = group.iter().map(|bin| bin.count).sum::<u64>() / 2;
        let mut walked = 0;
        let split = group
            .iter()
            .position(|bin| {
                walked += bin.count;
                walked >= half
            })
            .unwrap_or(0)
            + 1;
        let split = split.min(group.len() - 1);
        let right = group.split_off(split);
        groups.push(right);
    }
    groups
        .into_iter()
        .map(|group| {
            let mut total = Bucket::default();
            for bin in group {
                total.count += bin.count;
                for (sum, weight) in total.sum.iter_mut().zip(bin.sum) {
                    *sum += weight;
                }
            }
            total.color()
        })
        .collect()
}

fn widest(group: &[Bucket]) -> (usize, u8) {
    let mut min = [255; 4];
    let mut max = [0; 4];
    for bin in group {
        for (i, component) in bin.color().components().into_iter().enumerate() {
            min[i] = min[i].min(component);
            max[i] = max[i].max(component);
        }
    }
    min.into_iter()
        .zip(max)
        .map(|(minimum, maximum)| maximum - minimum)
        .enumerate()
        .max_by_key(|(_, width)| *width)
        .unwrap_or_default()
}

fn closest(palette: &[Color], color: Color) -> Option<u8> {
    let channels = color.components();
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, color)| {
            channels
                .into_iter()
                .zip(color.components())
                .map(|(a, b)| {
                    let difference = i32::from(a) - i32::from(b);
                    difference * difference
                })
                .sum::<i32>()
        })
        .map(|(index, _)| index as u8)
}
