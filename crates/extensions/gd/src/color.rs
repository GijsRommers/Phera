//! `imagecolorallocatealpha`, `imagecolorat` and GD's inverted seven-bit alpha.

use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, NativeError, NativeResult,
    Param as P, ParamType as T, PhpArray, Registry, Value, args,
};

use crate::core::{IMAGE, value_error, with, with_mut};
use crate::pixels::{Color, Storage};

const RED: P = P::new("red", T::Int);
const GREEN: P = P::new("green", T::Int);
const BLUE: P = P::new("blue", T::Int);
const ALPHA: P = P::new("alpha", T::Int);
const COLOR: P = P::new("color", T::Int);
const ENABLE: P = P::new("enable", T::Bool);
const X: P = P::new("x", T::Int);
const Y: P = P::new("y", T::Int);

pub fn register(registry: &mut Registry) {
    for (entry, result) in const {
        [
            (
                F::sync(
                    "imagecolorallocate",
                    &[IMAGE, RED, GREEN, BLUE],
                    |_, values| allocate(values, "imagecolorallocate", false),
                ),
                "int|false",
            ),
            (
                F::sync(
                    "imagecolorallocatealpha",
                    &[IMAGE, RED, GREEN, BLUE, ALPHA],
                    |_, values| allocate(values, "imagecolorallocatealpha", true),
                ),
                "int|false",
            ),
            (
                F::sync("imagecolorat", &[IMAGE, X, Y], color_at),
                "int|false",
            ),
            (
                F::sync("imagecolorsforindex", &[IMAGE, COLOR], colors_for_index),
                "array",
            ),
            (
                F::sync("imagecolorstotal", &[IMAGE], |_, values| {
                    with(values, 0, |image| {
                        let count = match &image.storage {
                            Storage::TrueColor(_) => 0,
                            Storage::Indexed { palette, .. } => palette.len() as i64,
                        };
                        Ok(Value::Int(count))
                    })
                }),
                "int",
            ),
            (
                F::sync(
                    "imagecolortransparent",
                    const { &[IMAGE, COLOR.nullable().with_default(C::Null)] },
                    transparent,
                ),
                "int",
            ),
            (
                F::sync("imagealphablending", &[IMAGE, ENABLE], |_, values| {
                    with_mut(values, 0, |image| {
                        image.blend = args::bool(values, 1).unwrap_or(false);
                        Ok(Value::Bool(true))
                    })
                }),
                "true",
            ),
            (
                F::sync("imagesavealpha", &[IMAGE, ENABLE], |_, values| {
                    with_mut(values, 0, |image| {
                        image.save_alpha = args::bool(values, 1).unwrap_or(false);
                        Ok(Value::Bool(true))
                    })
                }),
                "true",
            ),
            (
                F::sync("imagesetpixel", &[IMAGE, X, Y, COLOR], set_pixel),
                "true",
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

fn component(
    values: &[Value],
    name: &str,
    index: usize,
    parameter: &str,
    max: u8,
) -> Result<u8, NativeError> {
    let number = args::int(values, index).unwrap_or_default();
    if (0..=i64::from(max)).contains(&number) {
        Ok(number as u8)
    } else {
        Err(value_error(
            name,
            index + 1,
            parameter,
            &format!("must be between 0 and {max} (inclusive)"),
        ))
    }
}

fn allocate(values: &[Value], function: &str, with_alpha: bool) -> NativeResult {
    let red = component(values, function, 1, "red", 255)?;
    let green = component(values, function, 2, "green", 255)?;
    let blue = component(values, function, 3, "blue", 255)?;
    let alpha = if with_alpha {
        component(values, function, 4, "alpha", 127)?
    } else {
        0
    };
    with_mut(values, 0, |image| {
        Ok(image
            .allocate(Color::gd(red, green, blue, alpha))
            .map_or(Value::Bool(false), |index| Value::Int(i64::from(index))))
    })
}

fn color_at(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let (x, y) = (
        args::int(values, 1).unwrap_or_default(),
        args::int(values, 2).unwrap_or_default(),
    );
    with(values, 0, |image| {
        let Some(location) = image.location(x, y) else {
            context.notice(format!("imagecolorat(): {x},{y} is out of bounds"));
            return Ok(Value::Bool(false));
        };
        Ok(Value::Int(i64::from(image.index(location))))
    })
}

fn colors_for_index(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let index = args::int(values, 1).unwrap_or_default() as i32;
    with(values, 0, |image| {
        let color = image
            .color(index)
            .ok_or_else(|| value_error("imagecolorsforindex", 2, "color", "is out of range"))?;
        let mut result = PhpArray::new();
        for (name, value) in ["red", "green", "blue", "alpha"]
            .into_iter()
            .zip(color.components())
        {
            result.insert(ArrayKey::str(name), Value::Int(i64::from(value)));
        }
        Ok(Value::from(result))
    })
}

fn transparent(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        if let Some(color) = args::int(values, 1).map(|index| index as i32)
            && color >= 0
        {
            match &mut image.storage {
                Storage::TrueColor(_) => image.transparent = Some(color),
                Storage::Indexed { palette, .. } if (color as usize) < palette.len() => {
                    if let Some(old) = image.transparent {
                        palette[old as usize].0 &= 0x00ff_ffff;
                    }
                    palette[color as usize].0 |= Color::TRANSPARENT.0;
                    image.transparent = Some(color);
                }
                _ => {}
            }
        }
        Ok(Value::Int(i64::from(image.transparent.unwrap_or(-1))))
    })
}

fn set_pixel(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        let x = args::int(values, 1).unwrap_or_default() as i32;
        let y = args::int(values, 2).unwrap_or_default() as i32;
        let color = args::int(values, 3).unwrap_or_default() as i32;
        image.set(i64::from(x), i64::from(y), color);
        Ok(Value::Bool(true))
    })
}
