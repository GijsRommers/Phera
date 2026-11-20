//! Integer GD strokes and inclusive rectangles such as `imagefilledrectangle($im, 1, 1, 3, 3, 0)`.

use crate::pixels::{Color, Image};

use super::Point;

fn clip(a: &mut Point, b: &mut Point, max: i64) -> bool {
    if (a.x < 0 && b.x < 0) || (a.x > max && b.x > max) {
        return false;
    }
    let first = a.x.clamp(0, max);
    let last = b.x.clamp(0, max);
    if first != a.x || last != b.x {
        let gradient = (b.y - a.y) as f64 / (b.x - a.x) as f64;
        a.y += (gradient * (first - a.x) as f64) as i64;
        b.y += (gradient * (last - b.x) as f64) as i64;
        a.x = first;
        b.x = last;
    }
    true
}

pub(super) fn line(image: &mut Image, mut a: Point, mut b: Point, color: i32, aa: bool) {
    if !clip(&mut a, &mut b, i64::from(image.width) - 1) {
        return;
    }
    a = a.transpose();
    b = b.transpose();
    if !clip(&mut a, &mut b, i64::from(image.height) - 1) {
        return;
    }
    a = a.transpose();
    b = b.transpose();

    if a.y == b.y || a.x == b.x {
        let width = i64::from(image.thickness.max(1));
        if a.y == b.y {
            a.y -= width / 2;
            b.y = a.y + width - 1;
        } else {
            a.x -= width / 2;
            b.x = a.x + width - 1;
        }
        filled(image, a, b, color);
        return;
    }

    let transposed = (b.x - a.x).abs() < (b.y - a.y).abs();
    if transposed {
        a = a.transpose();
        b = b.transpose();
    }
    if a.x > b.x {
        std::mem::swap(&mut a, &mut b);
    }
    let dx = b.x - a.x;
    let dy = (b.y - a.y).abs();
    let direction = (b.y - a.y).signum();
    let put = |image: &mut Image, x, y, color| {
        if transposed {
            image.set(y, x, color)
        } else {
            image.set(x, y, color)
        }
    };
    if aa && image.is_true_color() && color >= 0 {
        let source = Color(color as u32);
        for x in a.x..=b.x {
            let step = (x - a.x) * dy;
            let y = a.y + direction * (step / dx);
            let second = ((step % dx) * 255 / dx) as u32;
            let plot = |image: &mut Image, y, coverage| {
                let point = if transposed {
                    Point { x: y, y: x }
                } else {
                    Point { x, y }
                };
                if let Some(location) = image.location(point.x, point.y) {
                    let prior = image.true_pixel(location).components();
                    let source = source.components();
                    let channel = |index: usize| -> u8 {
                        ((u32::from(source[index]) * coverage
                            + u32::from(prior[index]) * (255 - coverage))
                            / 255) as u8
                    };
                    put(
                        image,
                        x,
                        y,
                        Color::gd(channel(0), channel(1), channel(2), 0).0 as i32,
                    );
                }
            };
            plot(image, y, 255 - second);
            if second != 0 {
                plot(image, y + direction, second);
            }
        }
        return;
    }
    let slope = (dy as f64).atan2(dx as f64);
    let width = (f64::from(image.thickness) / slope.cos()) as i64;
    let width = if width == 0 { 1 } else { width };
    let max_y = i64::from(if transposed {
        image.width
    } else {
        image.height
    });
    let mut error = 2 * dy - dx;
    let mut center = a.y;
    for x in a.x..=b.x {
        let start = center - width / 2;
        for y in start.max(0)..(start + width).min(max_y) {
            put(image, x, y, color);
        }
        if error >= 0 {
            center += direction;
            error -= 2 * dx;
        }
        error += 2 * dy;
    }
}

pub(super) fn filled(image: &mut Image, a: Point, b: Point, color: i32) {
    let x = a.x.min(b.x).max(0);
    let y = a.y.min(b.y).max(0);
    let right = a.x.max(b.x).min(i64::from(image.width) - 1);
    let bottom = a.y.max(b.y).min(i64::from(image.height) - 1);
    for row in y..=bottom {
        span(image, row, x, right, color);
    }
}

pub(super) fn span(image: &mut Image, y: i64, left: i64, right: i64, color: i32) {
    if !(0..i64::from(image.height)).contains(&y) {
        return;
    }
    for x in left.max(0)..=right.min(i64::from(image.width) - 1) {
        image.set(x, y, color);
    }
}

pub(super) fn rectangle(image: &mut Image, a: Point, b: Point, color: i32) {
    let (left, right) = (a.x.min(b.x), a.x.max(b.x));
    let (top, bottom) = (a.y.min(b.y), a.y.max(b.y));
    let thickness = i64::from(image.thickness);
    if thickness > 1 {
        let half = thickness >> 1;
        let (left, right, top, bottom) = (left - half, right + half, top - half, bottom + half);
        filled(
            image,
            Point { x: left, y: top },
            Point {
                x: right,
                y: top + thickness - 1,
            },
            color,
        );
        filled(
            image,
            Point {
                x: left,
                y: bottom - thickness + 1,
            },
            Point {
                x: right,
                y: bottom,
            },
            color,
        );
        if top + thickness <= bottom - thickness {
            filled(
                image,
                Point {
                    x: left,
                    y: top + thickness,
                },
                Point {
                    x: left + thickness,
                    y: bottom - thickness,
                },
                color,
            );
            filled(
                image,
                Point {
                    x: right - thickness,
                    y: top + thickness,
                },
                Point {
                    x: right,
                    y: bottom - thickness,
                },
                color,
            );
        }
        return;
    }
    if left == right || top == bottom {
        line(image, a, b, color, false);
        return;
    }
    span(image, top, left, right, color);
    span(image, bottom, left, right, color);
    for y in (top + 1).max(0)..=(bottom - 1).min(i64::from(image.height) - 1) {
        image.set(left, y, color);
        image.set(right, y, color);
    }
}
