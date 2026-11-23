//! Ellipse and arc methods use GD's center, diameter, and degree arguments.

use phera_extension_api::{
    FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value,
};

use crate::core::{IMAGE, with_mut};
use crate::pixels::Image;

use super::{Point, int, polygons, stroke};

const CENTER_X: P = P::new("center_x", T::Int);
const CENTER_Y: P = P::new("center_y", T::Int);
const WIDTH: P = P::new("width", T::Int);
const HEIGHT: P = P::new("height", T::Int);
const START: P = P::new("start_angle", T::Int);
const END: P = P::new("end_angle", T::Int);
const COLOR: P = P::new("color", T::Int);
const STYLE: P = P::new("style", T::Int);
const ELLIPSE: &[P] = &[IMAGE, CENTER_X, CENTER_Y, WIDTH, HEIGHT, COLOR];
const ARC: &[P] = &[IMAGE, CENTER_X, CENTER_Y, WIDTH, HEIGHT, START, END, COLOR];

pub(super) fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::sync("imageellipse", ELLIPSE, |_, values| ellipse(values, false)),
            F::sync("imagefilledellipse", ELLIPSE, |_, values| {
                ellipse(values, true)
            }),
            F::sync("imagearc", ARC, |_, values| arc(values, 2)),
            F::sync(
                "imagefilledarc",
                &[
                    IMAGE, CENTER_X, CENTER_Y, WIDTH, HEIGHT, START, END, COLOR, STYLE,
                ],
                |_, values| arc(values, int(values, 8)),
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "true", None);
    }
}

fn ellipse(values: &[Value], fill: bool) -> NativeResult {
    with_mut(values, 0, |image| {
        let center = Point::read(values, 1);
        let width = i64::from(int(values, 3));
        let height = i64::from(int(values, 4));
        let color = int(values, 5);
        ellipse_pixels(image, center, width >> 1, height >> 1, color, fill);
        Ok(Value::Bool(true))
    })
}

fn ellipse_pixels(image: &mut Image, center: Point, a: i64, b: i64, color: i32, fill: bool) {
    // Match GD's checked integer radii and keep the midpoint loop bounded.
    if a < 0
        || b < 0
        || a * a > i64::from(i32::MAX)
        || b * b > i64::from(i32::MAX)
        || a * b * b > i64::from(i32::MAX)
        || b * a * a > i64::from(i32::MAX)
    {
        return;
    }
    let mut x = a;
    let mut y = 0;
    let dx = 2 * a * a;
    let dy = 2 * b * b;
    let mut error = a * b * b;
    let mut rx = 2 * error;
    let mut ry = 0;
    let mut previous_y = None;
    if fill {
        stroke::span(image, center.y, center.x - x, center.x + x, color);
    } else {
        image.set(center.x - x, center.y, color);
        image.set(center.x + x, center.y, color);
    }
    while x > 0 {
        if error > 0 {
            y += 1;
            ry += dx;
            error -= ry;
        }
        if error <= 0 {
            x -= 1;
            rx -= dy;
            error += rx;
        }
        if fill {
            if previous_y != Some(y) {
                stroke::span(image, center.y + y, center.x - x, center.x + x, color);
                stroke::span(image, center.y - y, center.x - x, center.x + x, color);
            }
            previous_y = Some(y);
        } else {
            for row in [center.y + y, center.y - y] {
                image.set(center.x - x, row, color);
                image.set(center.x + x, row, color);
            }
        }
    }
}

struct ArcStyle {
    chord: bool,
    outline: bool,
    edges: bool,
}

fn arc(values: &[Value], style: i32) -> NativeResult {
    let style = ArcStyle {
        chord: style & 1 != 0,
        outline: style & 2 != 0,
        edges: style & 4 != 0,
    };
    with_mut(values, 0, |image| {
        let center = Point::read(values, 1);
        let width = i64::from(int(values, 3));
        let height = i64::from(int(values, 4));
        let start = int(values, 5) % 360;
        let end = int(values, 6) % 360;
        let (start, stop) = if start == end {
            (0, 360)
        } else {
            let start = start.rem_euclid(360);
            (start, start + (end - start).rem_euclid(360))
        };
        let points: Vec<_> = (start..=stop)
            .map(|angle| {
                let angle = f64::from(angle % 360).to_radians();
                let horizontal = quantized_trig(angle.cos());
                let vertical = quantized_trig(angle.sin());
                Point {
                    x: center.x + horizontal * width / 2048,
                    y: center.y + vertical * height / 2048,
                }
            })
            .collect();
        let color = int(values, 7);
        draw_arc(image, center, &points, start, stop, color, style);
        Ok(Value::Bool(true))
    })
}

fn quantized_trig(value: f64) -> i64 {
    let scaled = value * 1024.0;
    let rounded = scaled.round();
    if (scaled - rounded).abs() < 1e-10 {
        rounded as i64
    } else {
        scaled as i64
    }
}

fn draw_arc(
    image: &mut Image,
    center: Point,
    points: &[Point],
    start: i32,
    stop: i32,
    color: i32,
    style: ArcStyle,
) {
    let (Some(&first), Some(&last)) = (points.first(), points.last()) else {
        return;
    };
    if style.outline {
        if style.chord {
            stroke::line(image, first, last, color, false);
        } else {
            for edge in points.windows(2) {
                stroke::line(image, edge[0], edge[1], color, false);
            }
        }
        if style.edges {
            stroke::line(image, center, last, color, false);
            stroke::line(image, center, first, color, false);
        }
    } else if style.chord {
        polygons::filled(image, &[first, last, center], color);
    } else {
        let mut outline = vec![center, first];
        for ((step, &point), &previous) in points.iter().enumerate().skip(1).zip(points) {
            let angle = start + step as i32;
            if point.y != previous.y {
                outline.push(point);
            } else if ((!(90..=270).contains(&angle) && point.x > previous.x)
                || (angle > 90 && angle < 270 && point.x < previous.x))
                && let Some(edge) = outline.last_mut()
            {
                edge.x = point.x;
            }
        }
        if stop - start < 360 {
            if outline.get(1).is_some_and(|start| *start != first) {
                outline.insert(1, first);
            }
            if outline.last().is_some_and(|end| *end != last) {
                outline.push(last);
            }
        }
        outline.push(center);
        polygons::filled(image, &outline, color);
    }
}
