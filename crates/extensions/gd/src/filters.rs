//! `imagefilter($image, IMG_FILTER_GRAYSCALE)` changes the existing image.

mod convolution;
mod effects;

use phera_extension_api::{
    Context, FunctionEntry as F, NativeError, NativeResult, Param as P, ParamType as T, Registry,
    Value, args, prepare_single_argument,
};

use crate::core::{IMAGE, value_error, with_mut};
use crate::pixels::{Color, Image, Storage};

pub fn register(registry: &mut Registry) {
    let filter = F::sync(
        "imagefilter",
        const {
            &[
                IMAGE,
                P::new("filter", T::Int),
                P::new("args", T::Untyped).variadic(),
            ]
        },
        filter,
    );
    registry.function(filter);
    registry.describe_function(filter.name, "bool", None);
    let gamma = F::sync(
        "imagegammacorrect",
        const {
            &[
                IMAGE,
                P::new("input_gamma", T::Float),
                P::new("output_gamma", T::Float),
            ]
        },
        gamma,
    );
    registry.function(gamma);
    registry.describe_function(gamma.name, "true", None);
    convolution::register(registry);
}

const FILTER: &str = "imagefilter";

fn filter(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mode = args::int(values, 1).unwrap_or(-1);
    let (minimum, maximum) = match mode {
        0 | 1 | 5..=9 => (2, 8),
        2 | 3 | 10 => (3, 3),
        4 => (5, 6),
        11 => (3, 4),
        12 => (4, 5),
        _ => {
            return Err(value_error(
                FILTER,
                2,
                "filter",
                "must be one of the IMG_FILTER_* filter constants",
            ));
        }
    };
    check_count(values.len(), minimum, maximum)?;
    let mut options = values[2..].to_vec();
    for (index, option) in options.iter_mut().enumerate() {
        let ty = match (mode, index) {
            (10, _) => T::Float,
            (11, 1) => T::Bool,
            (12, 2) => T::Array,
            _ => T::Int,
        };
        if matches!(mode, 0 | 1 | 5..=9) {
            break;
        }
        prepare_single_argument(FILTER, index + 2, &P::new("args", ty), option, context)
            .map_err(unnamed_option)?;
    }
    with_mut(values, 0, |image| {
        let first = args::int(&options, 0).unwrap_or_default() as i32;
        let applied = match mode {
            0 => {
                map_pixels(image, false, |[r, g, b, a]| {
                    Color::gd(255 - r, 255 - g, 255 - b, a)
                });
                true
            }
            1 => {
                map_pixels(image, true, |[r, g, b, a]| {
                    let gray =
                        (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)) as u8;
                    Color::gd(gray, gray, gray, a)
                });
                true
            }
            2 if !(-255..=255).contains(&first) => false,
            2 => {
                if first != 0 {
                    tint(image, [first, first, first, 0]);
                }
                true
            }
            3 => {
                let contrast = ((100.0 - f64::from(first)) / 100.0).powi(2);
                map_pixels(image, false, |[r, g, b, a]| {
                    let tone =
                        |v: u8| (((f64::from(v) / 255.0 - 0.5) * contrast + 0.5) * 255.0) as u8;
                    Color::gd(tone(r), tone(g), tone(b), a)
                });
                true
            }
            4 => {
                tint(
                    image,
                    std::array::from_fn(|index| {
                        args::int(&options, index).unwrap_or_default() as i32
                    }),
                );
                true
            }
            5 => convolution::apply(
                image,
                [[-1., 0., -1.], [0., 4., 0.], [-1., 0., -1.]],
                1.,
                127.,
            ),
            6 => convolution::apply(
                image,
                [[1.5, 0., 0.], [0., 0., 0.], [0., 0., -1.5]],
                1.,
                127.,
            ),
            7 => convolution::apply(image, [[1., 2., 1.], [2., 4., 2.], [1., 2., 1.]], 16., 0.),
            8 => convolution::selective(image),
            9 => convolution::apply(
                image,
                [[-1., -1., -1.], [-1., 9., -1.], [-1., -1., -1.]],
                1.,
                0.,
            ),
            10 => {
                let weight = args::float(&options, 0).unwrap_or_default() as f32;
                convolution::apply(
                    image,
                    [[1., 1., 1.], [1., weight, 1.], [1., 1., 1.]],
                    weight + 8.,
                    0.,
                )
            }
            11 => effects::pixelate(image, first, args::bool(&options, 1).unwrap_or(false)),
            12 => return effects::scatter(context, image, &options),
            _ => false,
        };
        Ok(Value::Bool(applied))
    })
}

fn unnamed_option(error: NativeError) -> NativeError {
    match error {
        NativeError::Throw {
            class,
            message,
            code,
        } => NativeError::Throw {
            class,
            message: message.replace(" ($args)", ""),
            code,
        },
        other => other,
    }
}

fn check_count(count: usize, minimum: usize, maximum: usize) -> Result<(), NativeError> {
    if (minimum..=maximum).contains(&count) {
        return Ok(());
    }
    let expected = if minimum == maximum {
        format!("exactly {minimum}")
    } else if count < minimum {
        format!("at least {minimum}")
    } else {
        format!("at most {maximum}")
    };
    Err(NativeError::argument_count_error(format!(
        "{FILTER}() expects {expected} arguments, {count} given"
    )))
}

fn tint(image: &mut Image, amount: [i32; 4]) {
    map_pixels(image, false, |channels| {
        let [r, g, b, a] = std::array::from_fn(|i| {
            let limit = if i == 3 { 127 } else { 255 };
            i32::from(channels[i])
                .saturating_add(amount[i])
                .clamp(0, limit) as u8
        });
        Color::gd(r, g, b, a)
    });
}

fn map_pixels(image: &mut Image, replace: bool, transform: impl Fn([u8; 4]) -> Color) {
    let blend = image.blend;
    image.blend &= !replace;
    let count = image.width as usize * image.height as usize;
    for position in 0..count {
        let color = transform(image.true_pixel(position).components());
        store_color(image, position, color);
    }
    image.blend = blend;
}

fn store_color(image: &mut Image, position: usize, color: Color) {
    let index = image
        .allocate(color)
        .unwrap_or_else(|| image.closest(color));
    image.set_at(position, index);
}

fn gamma(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let input = args::float(values, 1).unwrap_or_default();
    let output = args::float(values, 2).unwrap_or_default();
    for (value, index, parameter) in [(input, 2, "input_gamma"), (output, 3, "output_gamma")] {
        if value <= 0.0 || !value.is_finite() {
            let reason = if value <= 0.0 {
                "must be greater than 0"
            } else {
                "must be finite"
            };
            return Err(value_error("imagegammacorrect", index, parameter, reason));
        }
    }
    let exponent = input / output;
    let table: [u8; 256] =
        std::array::from_fn(|value| ((value as f64 / 255.0).powf(exponent) * 255.0 + 0.5) as u8);
    let transform = |[r, g, b, a]: [u8; 4]| {
        Color::gd(
            table[usize::from(r)],
            table[usize::from(g)],
            table[usize::from(b)],
            a,
        )
    };
    with_mut(values, 0, |image| {
        if let Storage::Indexed { palette, .. } = &mut image.storage {
            for color in palette {
                *color = transform(color.components());
            }
        } else {
            map_pixels(image, false, transform);
        }
        Ok(Value::Bool(true))
    })
}
