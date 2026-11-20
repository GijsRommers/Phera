//! `imagechar` draws one byte from the same packed bitmaps used by PHP.

use phera_extension_api::{
    Context, FunctionEntry as F, NativeError, NativeResult, Param as P, ParamType as T, Registry,
    Value, args, prepare_single_argument,
};

use crate::core::{IMAGE, with_mut};
use crate::pixels::Image;

const FONT: P = P::new("font", T::Untyped);
const X: P = P::new("x", T::Int);
const Y: P = P::new("y", T::Int);
const CHAR: P = P::new("char", T::String);
const STRING: P = P::new("string", T::String);
const COLOR: P = P::new("color", T::Int);

pub(super) fn register(registry: &mut Registry) {
    for (entry, result) in const {
        [
            (
                F::sync("imagefontwidth", &[FONT], |context, args| {
                    Ok(Value::Int(font(context, args, "imagefontwidth", 0)?.width))
                }),
                "int",
            ),
            (
                F::sync("imagefontheight", &[FONT], |context, args| {
                    Ok(Value::Int(
                        font(context, args, "imagefontheight", 0)?.height,
                    ))
                }),
                "int",
            ),
            (
                F::sync(
                    "imagechar",
                    &[IMAGE, FONT, X, Y, CHAR, COLOR],
                    |context, args| paint(context, args, "imagechar", Text::Character),
                ),
                "true",
            ),
            (
                F::sync(
                    "imagecharup",
                    &[IMAGE, FONT, X, Y, CHAR, COLOR],
                    |context, args| paint(context, args, "imagecharup", Text::CharacterUp),
                ),
                "true",
            ),
            (
                F::sync(
                    "imagestring",
                    &[IMAGE, FONT, X, Y, STRING, COLOR],
                    |context, args| paint(context, args, "imagestring", Text::String),
                ),
                "true",
            ),
            (
                F::sync(
                    "imagestringup",
                    &[IMAGE, FONT, X, Y, STRING, COLOR],
                    |context, args| paint(context, args, "imagestringup", Text::StringUp),
                ),
                "true",
            ),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, result, None);
    }
}

struct BitmapFont {
    width: i64,
    height: i64,
    glyphs: &'static [u8],
}

const FONTS: &[BitmapFont] = &[
    BitmapFont {
        width: 5,
        height: 8,
        glyphs: include_bytes!("../../data/font-tiny.bin"),
    },
    BitmapFont {
        width: 6,
        height: 13,
        glyphs: include_bytes!("../../data/font-small.bin"),
    },
    BitmapFont {
        width: 7,
        height: 13,
        glyphs: include_bytes!("../../data/font-medium-bold.bin"),
    },
    BitmapFont {
        width: 8,
        height: 16,
        glyphs: include_bytes!("../../data/font-large.bin"),
    },
    BitmapFont {
        width: 9,
        height: 15,
        glyphs: include_bytes!("../../data/font-giant.bin"),
    },
];

fn font(
    context: &mut dyn Context,
    values: &[Value],
    name: &str,
    index: usize,
) -> Result<&'static BitmapFont, NativeError> {
    let mut argument = args::value(values, index).clone();
    if let Err(error) =
        prepare_single_argument(name, index, &P::new("font", T::Int), &mut argument, context)
    {
        let label = args::value(values, index).type_name();
        return match error {
            NativeError::Throw {
                class: "TypeError", ..
            } => Err(NativeError::type_error(format!(
                "{name}(): Argument #{} ($font) must be of type GdFont|int, {label} given",
                index + 1
            ))),
            _ => Err(error),
        };
    }
    let id = args::int(&[argument], 0).unwrap_or(1).clamp(1, 5);
    Ok(&FONTS[id as usize - 1])
}

enum Text {
    Character,
    CharacterUp,
    String,
    StringUp,
}

fn paint(context: &mut dyn Context, values: &[Value], name: &str, mode: Text) -> NativeResult {
    let font = font(context, values, name, 1)?;
    let mut x = i64::from(args::int(values, 2).unwrap_or_default() as i32);
    let mut y = i64::from(args::int(values, 3).unwrap_or_default() as i32);
    let bytes = args::bytes(values, 4);
    let color = args::int(values, 5).unwrap_or_default() as i32;
    let up = matches!(mode, Text::CharacterUp | Text::StringUp);
    with_mut(values, 0, |image| {
        if matches!(mode, Text::Character | Text::CharacterUp) {
            let character = bytes.first().copied().unwrap_or_default();
            glyph(image, font, character, (x, y), color, up);
        } else {
            for &character in bytes.iter().take_while(|byte| **byte != 0) {
                if (!up && x >= i64::from(image.width)) || (up && y < 0) {
                    break;
                }
                glyph(image, font, character, (x, y), color, up);
                if up {
                    y -= font.width;
                } else {
                    x += font.width;
                }
            }
        }
        Ok(Value::Bool(true))
    })
}

fn glyph(image: &mut Image, font: &BitmapFont, byte: u8, origin: (i64, i64), color: i32, up: bool) {
    let offset = i64::from(byte) * font.height * font.width;
    for row in 0..font.height {
        for column in 0..font.width {
            let bit = (offset + row * font.width + column) as usize;
            if font.glyphs[bit / 8] >> (bit % 8) & 1 != 0 {
                if up {
                    image.set(origin.0 + row, origin.1 - column, color);
                } else {
                    image.set(origin.0 + column, origin.1 + row, color);
                }
            }
        }
    }
}
