//! Copy an image region with GD's nearest-neighbor and area-weighted resizing.

use std::rc::Rc;

use phera_extension_api::{
    FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value, args,
};

use crate::core::{with, with_mut};
use crate::pixels::{Color, Image};

const IMAGE: T = T::Class("GdImage");
const DST: P = P::new("dst_image", IMAGE);
const SRC: P = P::new("src_image", IMAGE);
const DX: P = P::new("dst_x", T::Int);
const DY: P = P::new("dst_y", T::Int);
const SX: P = P::new("src_x", T::Int);
const SY: P = P::new("src_y", T::Int);
const DW: P = P::new("dst_width", T::Int);
const DH: P = P::new("dst_height", T::Int);
const SW: P = P::new("src_width", T::Int);
const SH: P = P::new("src_height", T::Int);
const PERCENT: P = P::new("pct", T::Int);
const RECTS: &[P] = &[DST, SRC, DX, DY, SX, SY, DW, DH, SW, SH];

pub(super) fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::sync(
                "imagecopy",
                &[DST, SRC, DX, DY, SX, SY, SW, SH],
                |_, values| copy(values, Sampling::Unscaled),
            ),
            F::sync(
                "imagecopymerge",
                &[DST, SRC, DX, DY, SX, SY, SW, SH, PERCENT],
                |_, values| copy(values, Sampling::Merge(number(values, 8) as i32)),
            ),
            F::sync("imagecopyresized", RECTS, |_, values| {
                copy(values, Sampling::Nearest)
            }),
            F::sync("imagecopyresampled", RECTS, |_, values| {
                copy(values, Sampling::Area)
            }),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "true", None);
    }
}

#[derive(Clone, Copy)]
enum Sampling {
    Unscaled,
    Nearest,
    Area,
    Merge(i32),
}

fn number(values: &[Value], index: usize) -> i64 {
    i64::from(args::int(values, index).unwrap_or_default() as i32)
}

#[derive(Clone, Copy)]
pub(super) struct Region {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
}

fn copy(values: &[Value], sampling: Sampling) -> NativeResult {
    let (sw, sh) = if matches!(sampling, Sampling::Unscaled | Sampling::Merge(_)) {
        (6, 7)
    } else {
        (8, 9)
    };
    let target = Region {
        x: number(values, 2),
        y: number(values, 3),
        width: number(values, 6),
        height: number(values, 7),
    };
    let source = Region {
        x: number(values, 4),
        y: number(values, 5),
        width: number(values, sw),
        height: number(values, sh),
    };
    let same = matches!((args::object(values, 0), args::object(values, 1)),
        (Some(destination), Some(source)) if Rc::ptr_eq(destination, source));
    if same {
        with_mut(values, 0, |image| {
            blit(image, None, target, source, sampling);
            Ok(Value::Bool(true))
        })
    } else {
        with(values, 1, |src| {
            with_mut(values, 0, |dst| {
                blit(dst, Some(src), target, source, sampling);
                Ok(Value::Bool(true))
            })
        })
    }
}

pub(super) fn unscaled(dst: &mut Image, src: &Image, target: Region, source: Region) {
    blit(dst, Some(src), target, source, Sampling::Unscaled);
}

fn blit(dst: &mut Image, src: Option<&Image>, target: Region, source: Region, sampling: Sampling) {
    if target.width <= 0 || target.height <= 0 || source.width <= 0 || source.height <= 0 {
        return;
    }
    let area = matches!(sampling, Sampling::Area) && dst.is_true_color();
    let min_x = target.x.max(0);
    let min_y = target.y.max(0);
    let max_x = (target.x + target.width).min(i64::from(dst.width));
    let max_y = (target.y + target.height).min(i64::from(dst.height));
    for y in min_y..max_y {
        for x in min_x..max_x {
            let source_image = src.unwrap_or(dst);
            if area {
                let color = weighted(source_image, source, target, x, y);
                dst.set(x, y, color.0 as i32);
                continue;
            }
            let sx = source.x + nearest(x - target.x, source.width, target.width);
            let sy = source.y + nearest(y - target.y, source.height, target.height);
            let index = source_image
                .location(sx, sy)
                .map_or(0, |pos| source_image.index(pos));
            if source_image.transparent == Some(index) {
                continue;
            }
            if src.is_none() {
                dst.set(x, y, index);
                continue;
            }
            let mut color = source_image.color(index).unwrap_or(Color::BLACK);
            if let Sampling::Merge(percent) = sampling {
                let prior = dst
                    .location(x, y)
                    .map_or(Color::BLACK, |pos| dst.true_pixel(pos));
                color = merge(color, prior, percent);
            }
            let index = dst.resolve(color);
            dst.set(x, y, index);
        }
    }
}

fn nearest(offset: i64, from: i64, to: i64) -> i64 {
    ((offset + 1) * from - 1) / to
}

fn merge(source: Color, destination: Color, percent: i32) -> Color {
    let source = source.components();
    let destination = destination.components();
    let weight = f64::from(percent) / 100.0;
    let prior = (100.0 - f64::from(percent)) / 100.0;
    let channel = |index| {
        let a = f64::from(source[index]) * weight;
        let b = f64::from(destination[index]) * prior;
        (a + b) as u8
    };
    Color::gd(channel(0), channel(1), channel(2), 0)
}

fn weighted(source: &Image, from: Region, to: Region, x: i64, y: i64) -> Color {
    let ratio_x = from.width as f64 / to.width as f64;
    let ratio_y = from.height as f64 / to.height as f64;
    let left = (x - to.x) as f64 * ratio_x;
    let right = (x - to.x + 1) as f64 * ratio_x;
    let top = (y - to.y) as f64 * ratio_y;
    let bottom = (y - to.y + 1) as f64 * ratio_y;
    let mut color = [0.0; 3];
    let mut transparency = 0.0;
    let mut opacity = 0.0;
    let mut covered = 0.0;
    let min_x = (left.floor() as i64 + from.x).max(0);
    let min_y = (top.floor() as i64 + from.y).max(0);
    let max_x = (right.ceil() as i64 + from.x).min(i64::from(source.width));
    let max_y = (bottom.ceil() as i64 + from.y).min(i64::from(source.height));
    for sy in min_y..max_y {
        let row = (sy - from.y) as f64;
        let vertical = (row + 1.0).min(bottom) - row.max(top);
        for sx in min_x..max_x {
            let column = (sx - from.x) as f64;
            let horizontal = (column + 1.0).min(right) - column.max(left);
            let coverage = vertical * horizontal;
            let Some(location) = source.location(sx, sy) else {
                continue;
            };
            let [r, g, b, alpha] = source.true_pixel(location).components().map(f64::from);
            let weight = (127.0 - alpha) * coverage;
            for (part, component) in color.iter_mut().zip([r, g, b]) {
                *part += component * weight;
            }
            transparency += alpha * coverage;
            opacity += weight;
            covered += coverage;
        }
    }
    let total = (right - left) * (bottom - top);
    opacity += 127.0 * (total - covered).max(0.0);
    let [r, g, b] = color.map(|channel| (channel / opacity.max(f64::MIN_POSITIVE)).round() as u8);
    Color::gd(r, g, b, (transparency / total).round() as u8)
}
