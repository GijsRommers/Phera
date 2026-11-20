//! GD drawing, e.g. `imageline($image, 0, 0, 8, 3, $red)`.

mod curved;
mod polygons;
mod stroke;

use phera_extension_api::{
    Context, FunctionEntry as F, NativeResult, Param as P, ParamType as T, Registry, Value, args,
};

use crate::core::{IMAGE, with_mut};
use crate::pixels::Image;

const COLOR: P = P::new("color", T::Int);
const PAINT: &[P] = &[IMAGE, P::new("x", T::Int), P::new("y", T::Int), COLOR];
const ENDPOINTS: &[P] = &[
    IMAGE,
    P::new("x1", T::Int),
    P::new("y1", T::Int),
    P::new("x2", T::Int),
    P::new("y2", T::Int),
    COLOR,
];

pub fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::sync(
                "imagesetthickness",
                const { &[IMAGE, P::new("thickness", T::Int)] },
                thickness,
            ),
            F::sync(
                "imageantialias",
                const { &[IMAGE, P::new("enable", T::Bool)] },
                antialias,
            ),
            F::sync("imageline", ENDPOINTS, |_, values| {
                shape(values, |image, first, last, color| {
                    stroke::line(image, first, last, color, image.antialias);
                })
            }),
            F::sync("imagerectangle", ENDPOINTS, |_, values| {
                shape(values, stroke::rectangle)
            }),
            F::sync("imagefilledrectangle", ENDPOINTS, |_, values| {
                shape(values, stroke::filled)
            }),
            F::sync("imagefill", PAINT, fill),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "true", None);
    }
    curved::register(registry);
    polygons::register(registry);
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i64,
    y: i64,
}

impl Point {
    fn read(values: &[Value], offset: usize) -> Self {
        Self {
            x: int(values, offset).into(),
            y: int(values, offset + 1).into(),
        }
    }

    fn transpose(self) -> Self {
        Self {
            x: self.y,
            y: self.x,
        }
    }
}

fn int(values: &[Value], index: usize) -> i32 {
    args::int(values, index).unwrap_or_default() as i32
}

fn shape(values: &[Value], draw: impl FnOnce(&mut Image, Point, Point, i32)) -> NativeResult {
    with_mut(values, 0, |image| {
        draw(
            image,
            Point::read(values, 1),
            Point::read(values, 3),
            int(values, 5),
        );
        Ok(Value::Bool(true))
    })
}

fn thickness(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        image.thickness = int(values, 1);
        Ok(Value::Bool(true))
    })
}

fn antialias(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        image.antialias = args::bool(values, 1).unwrap_or(false);
        Ok(Value::Bool(true))
    })
}

fn fill(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        let point = Point::read(values, 1);
        let color = int(values, 3);
        let Some(start) = image.location(point.x, point.y) else {
            return Ok(Value::Bool(true));
        };
        let original = image.index(start);
        let replacement = if image.is_true_color() {
            color
        } else {
            i32::from(color as u8)
        };
        if color < 0 || original == replacement {
            return Ok(Value::Bool(true));
        }
        let blend = std::mem::replace(&mut image.blend, false);
        image.set_at(start, color);
        let mut pending = vec![point];
        while let Some(Point { x, y }) = pending.pop() {
            for point in [
                Point { x: x - 1, y },
                Point { x: x + 1, y },
                Point { x, y: y - 1 },
                Point { x, y: y + 1 },
            ] {
                if let Some(location) = image.location(point.x, point.y)
                    && image.index(location) == original
                {
                    image.set_at(location, color);
                    pending.push(point);
                }
            }
        }
        image.blend = blend;
        Ok(Value::Bool(true))
    })
}
