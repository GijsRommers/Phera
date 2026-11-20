//! `$image = imagecreatetruecolor(3, 2); echo imagesx($image);`

use std::cell::RefCell;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue as C, Context, FunctionEntry as F,
    NativeComparison, NativeError, NativePropertyAccess, NativePropertyMutation, NativeResult,
    Param as P, ParamType as T, PhpArray, Registry, Value, args,
};

use crate::pixels::{Image, MAX_PIXELS};

pub const IMAGE: P = P::new("image", T::Class("GdImage"));
const SIZE: &[P] = &[P::new("width", T::Int), P::new("height", T::Int)];
const ENABLE: P = P::new("enable", T::Bool).nullable().with_default(C::Null);
const RESOLUTION_X: P = P::new("resolution_x", T::Int)
    .nullable()
    .with_default(C::Null);
const RESOLUTION_Y: P = P::new("resolution_y", T::Int)
    .nullable()
    .with_default(C::Null);

const NO_DYNAMIC_PROPERTIES: NativePropertyAccess = NativePropertyAccess::new(
    |_, _, _, _| Ok(None),
    |_, _, name, mutation| match mutation {
        NativePropertyMutation::Unset => Ok(false),
        _ => Err(NativeError::error(format!(
            "Cannot create dynamic property GdImage::${name}"
        ))),
    },
);

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(&ClassHooks {
            construction_error: Some(
                "You cannot initialize a GdImage object except through helper functions",
            ),
            clone: Some(phera_extension_api::NativeClone::Forbidden),
            compare: Some(|left, right, _| {
                Ok(if std::ptr::eq(left, right) {
                    NativeComparison::Ordered(std::cmp::Ordering::Equal)
                } else {
                    NativeComparison::Uncomparable
                })
            }),
            forbid_serialization: true,
            property_access: Some(&NO_DYNAMIC_PROPERTIES),
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class("GdImage")
    });
    for (function, result) in const {
        [
            (
                F::sync("imagecreatetruecolor", SIZE, |context, values| {
                    create(context, values, true, "imagecreatetruecolor")
                }),
                "GdImage|false",
            ),
            (
                F::sync("imagecreate", SIZE, |context, values| {
                    create(context, values, false, "imagecreate")
                }),
                "GdImage|false",
            ),
            (
                F::sync("imagesx", &[IMAGE], |_, values| {
                    with(values, 0, |image| Ok(Value::from(i64::from(image.width))))
                }),
                "int",
            ),
            (
                F::sync("imagesy", &[IMAGE], |_, values| {
                    with(values, 0, |image| Ok(Value::from(i64::from(image.height))))
                }),
                "int",
            ),
            (
                F::sync("imageistruecolor", &[IMAGE], |_, values| {
                    with(values, 0, |image| Ok(Value::Bool(image.is_true_color())))
                }),
                "bool",
            ),
            (
                F::sync("imagepalettetotruecolor", &[IMAGE], |_, values| {
                    with_mut(values, 0, |image| {
                        Ok(Value::Bool(image.palette_to_true_color()))
                    })
                }),
                "bool",
            ),
            (
                F::deprecated_sync("imagedestroy", &[IMAGE], |_, _| Ok(Value::Bool(true))),
                "true",
            ),
            (
                F::sync("imageinterlace", &[IMAGE, ENABLE], interlace),
                "bool",
            ),
            (
                F::sync(
                    "imageresolution",
                    &[IMAGE, RESOLUTION_X, RESOLUTION_Y],
                    resolution,
                ),
                "array|true",
            ),
        ]
    } {
        registry.function(function);
        registry.describe_function(function.name, result, None);
    }
}

fn create(
    context: &mut dyn Context,
    values: &[Value],
    true_color: bool,
    name: &str,
) -> NativeResult {
    let width = dimension(values, 0, name, "width")?;
    let height = dimension(values, 1, name, "height")?;
    let Some(image) = Image::create(width, height, true_color) else {
        context.warning(format!(
            "{name}(): Image dimensions exceed the {MAX_PIXELS} pixel limit"
        ));
        return Ok(Value::Bool(false));
    };
    wrap(context, image)
}

pub fn dimension(
    values: &[Value],
    index: usize,
    name: &str,
    parameter: &str,
) -> Result<u32, NativeError> {
    let number = args::int(values, index).unwrap_or_default();
    if (1..i64::from(i32::MAX)).contains(&number) {
        return Ok(number as u32);
    }
    Err(value_error(
        name,
        index + 1,
        parameter,
        "must be greater than 0",
    ))
}

pub fn value_error(name: &str, index: usize, parameter: &str, message: &str) -> NativeError {
    NativeError::value_error(format!(
        "{name}(): Argument #{index} (${parameter}) {message}"
    ))
}

pub fn wrap(context: &mut dyn Context, image: Image) -> NativeResult {
    let value = context.create_object("GdImage", PhpArray::new())?;
    if let Value::Object(object) = &value {
        *object.native.borrow_mut() = Some(Box::new(RefCell::new(image)));
    }
    Ok(value)
}

fn state_error() -> NativeError {
    NativeError::error("Invalid GdImage object")
}

pub fn with<T>(
    values: &[Value],
    index: usize,
    run: impl FnOnce(&Image) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    access(values, index, |cell| {
        let state = cell.try_borrow().map_err(|_| state_error())?;
        run(&state)
    })
}

pub fn with_mut<T>(
    values: &[Value],
    index: usize,
    run: impl FnOnce(&mut Image) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    access(values, index, |cell| {
        let mut state = cell.try_borrow_mut().map_err(|_| state_error())?;
        run(&mut state)
    })
}

fn access<T>(
    values: &[Value],
    index: usize,
    run: impl FnOnce(&RefCell<Image>) -> Result<T, NativeError>,
) -> Result<T, NativeError> {
    let object = args::object(values, index).ok_or_else(state_error)?;
    let native = object.native.try_borrow().map_err(|_| state_error())?;
    let cell = native
        .as_ref()
        .and_then(|data| data.downcast_ref::<RefCell<Image>>())
        .ok_or_else(state_error)?;
    run(cell)
}

fn interlace(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    with_mut(values, 0, |image| {
        if let Some(enable) = args::bool(values, 1) {
            image.interlaced = enable;
        }
        Ok(Value::Bool(image.interlaced))
    })
}

fn resolution(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let read_dpi = |index, parameter| {
        args::int(values, index)
            .map(|value| {
                u32::try_from(value).map_err(|_| {
                    value_error(
                        "imageresolution",
                        index + 1,
                        parameter,
                        "must be between 0 and 4294967295",
                    )
                })
            })
            .transpose()
    };
    let (x, y) = (read_dpi(1, "resolution_x")?, read_dpi(2, "resolution_y")?);
    with_mut(values, 0, |image| {
        if let Some(dpi) = x.or(y) {
            for (target, source) in image.resolution.iter_mut().zip([x, y]) {
                let source = source.unwrap_or(dpi);
                if source != 0 {
                    *target = source;
                }
            }
            return Ok(Value::Bool(true));
        }
        let mut result = PhpArray::new();
        for dpi in image.resolution {
            result.push(Value::Int(i64::from(dpi)))?;
        }
        Ok(Value::from(result))
    })
}
