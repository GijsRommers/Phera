//! `imagefilledpolygon($image, [0, 0, 5, 0, 5, 5], $color)` uses even-odd scanlines.

use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, NativeError, NativeResult,
    Param as P, ParamType as T, Registry, Value, args, value::to_int,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::Image;

use super::{Point, stroke};

const PARAMS: &[P] = &[
    IMAGE,
    P::new("points", T::Array),
    P::new("num_points_or_color", T::Int),
    P::new("color", T::Int).nullable().with_default(C::Null),
];

pub(super) fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::sync("imagepolygon", PARAMS, |context, values| {
                polygon(context, values, "imagepolygon", Polygon::Closed)
            }),
            F::sync("imageopenpolygon", PARAMS, |context, values| {
                polygon(context, values, "imageopenpolygon", Polygon::Open)
            }),
            F::sync("imagefilledpolygon", PARAMS, |context, values| {
                polygon(context, values, "imagefilledpolygon", Polygon::Filled)
            }),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "bool", None);
    }
}

enum Polygon {
    Closed,
    Open,
    Filled,
}

fn polygon(
    context: &mut dyn Context,
    values: &[Value],
    name: &str,
    polygon: Polygon,
) -> NativeResult {
    let points = args::array(values, 1).ok_or_else(|| NativeError::error("Missing points"))?;
    let count_or_color = args::int(values, 2).unwrap_or_default() as i32;
    let (count, color) = if let Some(color) = args::int(values, 3) {
        context.deprecated(format!(
            "{name}(): Using the $num_points parameter is deprecated"
        ));
        (i64::from(count_or_color), color as i32)
    } else {
        if points.len() % 2 != 0 {
            return Err(value_error(
                name,
                2,
                "points",
                "must have an even number of elements",
            ));
        }
        ((points.len() / 2) as i64, count_or_color)
    };
    if count < 3 {
        return Err(value_error(
            name,
            3,
            "num_points_or_color",
            "must be greater than or equal to 3",
        ));
    }
    if count as usize > points.len() / 2 {
        return Err(NativeError::value_error(format!(
            "Trying to use {count} points in array with only {} points",
            points.len() / 2
        )));
    }
    let mut outline = Vec::with_capacity(count as usize);
    for index in 0..count {
        let mut coordinate = |offset| -> Result<i64, NativeError> {
            let value = points
                .get(&ArrayKey::Int(index * 2 + offset))
                .unwrap_or(&Value::Null);
            Ok(i64::from(to_int(value, context.diagnostics())? as i32))
        };
        outline.push(Point {
            x: coordinate(0)?,
            y: coordinate(1)?,
        });
    }
    with_mut(values, 0, |image| {
        match polygon {
            Polygon::Closed => draw_edges(image, &outline, color, true),
            Polygon::Open => draw_edges(image, &outline, color, false),
            Polygon::Filled => {
                filled(image, &outline, color);
                if image.antialias {
                    draw_edges(image, &outline, color, true);
                }
            }
        }
        Ok(Value::Bool(true))
    })
}

fn draw_edges(image: &mut Image, points: &[Point], color: i32, closed: bool) {
    if closed && let (Some(last), Some(first)) = (points.last(), points.first()) {
        stroke::line(image, *last, *first, color, image.antialias);
    }
    for edge in points.windows(2) {
        stroke::line(image, edge[0], edge[1], color, image.antialias);
    }
}

pub(super) fn filled(image: &mut Image, points: &[Point], color: i32) {
    let Some(min_y) = points.iter().map(|point| point.y).min() else {
        return;
    };
    let max_y = points.iter().map(|point| point.y).max().unwrap_or(min_y);
    if min_y == max_y {
        let min_x = points.iter().map(|point| point.x).min().unwrap_or(0);
        let max_x = points.iter().map(|point| point.x).max().unwrap_or(0);
        stroke::line(
            image,
            Point { x: min_x, y: min_y },
            Point { x: max_x, y: min_y },
            color,
            false,
        );
        return;
    }
    let mut crossings = Vec::with_capacity(points.len());
    for y in min_y.max(0)..=max_y.min(i64::from(image.height) - 1) {
        crossings.clear();
        for (&first, &last) in points.iter().zip(points.iter().cycle().skip(1)) {
            let (a, b) = if first.y < last.y {
                (first, last)
            } else {
                (last, first)
            };
            if y >= a.y && y < b.y {
                let rise = (y - a.y) as f32;
                let run = (b.x - a.x) as f32;
                let height = (b.y - a.y) as f32;
                crossings.push((f64::from(rise * run / height) + 0.5 + a.x as f64) as i64);
            } else if a.y != b.y && y == max_y && y == b.y {
                crossings.push(b.x);
            }
        }
        crossings.sort_unstable();
        for pair in crossings.chunks_exact(2) {
            stroke::line(
                image,
                Point { x: pair[0], y },
                Point { x: pair[1], y },
                color,
                false,
            );
        }
    }
}
