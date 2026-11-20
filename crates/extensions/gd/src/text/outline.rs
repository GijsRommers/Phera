//! `imagettfbbox(14, 0, 'font.ttf', 'Hi')` lays out glyphs using a Rust rasterizer.

mod path;

use std::io::Read;

use ab_glyph_rasterizer::{Point, point};
use phera_ext_streams::file_io::{blocking, resolve};
use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, LocalFuture, NativeResult,
    Param as P, ParamType as T, PhpArray, Registry, Value, args, value::to_float,
};

use crate::core::{IMAGE, with_mut};
use crate::pixels::{Color, Image};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    outline::DrawSettings,
};

use self::path::Path;

const SIZE: P = P::new("size", T::Float);
const ANGLE: P = P::new("angle", T::Float);
const X: P = P::new("x", T::Int);
const Y: P = P::new("y", T::Int);
const COLOR: P = P::new("color", T::Int);
const FONT: P = P::new("font_filename", T::String).without_nul();
const TEXT: P = P::new("text", T::String);
const STRING: P = P::new("string", T::String);
const OPTIONS: P = P::new("options", T::Array).with_default(C::EmptyArray);
const BBOX: &[P] = &[SIZE, ANGLE, FONT, STRING, OPTIONS];
const PAINT: &[P] = &[IMAGE, SIZE, ANGLE, X, Y, COLOR, FONT, TEXT, OPTIONS];
const FONT_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn register(registry: &mut Registry) {
    for entry in const {
        [
            F::asynchronous("imagettfbbox", BBOX, |cx, args| {
                run(cx, args, "imagettfbbox", false)
            }),
            F::asynchronous("imageftbbox", BBOX, |cx, args| {
                run(cx, args, "imageftbbox", false)
            }),
            F::asynchronous("imagettftext", PAINT, |cx, args| {
                run(cx, args, "imagettftext", true)
            }),
            F::asynchronous("imagefttext", PAINT, |cx, args| {
                run(cx, args, "imagefttext", true)
            }),
        ]
    } {
        registry.function(entry);
        registry.describe_function(entry.name, "array|false", None);
    }
}

fn run<'a>(
    context: &'a mut dyn Context,
    values: Vec<Value>,
    name: &'static str,
    draw: bool,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let start = usize::from(draw);
        let font_index = if draw { 6 } else { 2 };
        let [font, text] = [
            args::bytes(&values, font_index),
            args::bytes(&values, font_index + 1),
        ];
        let size = args::float(&values, start).unwrap_or_default() as f32;
        let angle = args::float(&values, start + 1).unwrap_or_default() as f32;
        let line_space = args::array(&values, font_index + 2)
            .and_then(|options| options.get(&ArrayKey::str("linespacing")))
            .map(|value| to_float(value, context.diagnostics()))
            .transpose()?
            .unwrap_or(1.05) as f32;
        let font = if font.is_empty() {
            None
        } else {
            resolve(context, font).ok()
        };
        let text = String::from_utf8_lossy(text).into_owned();
        let Some(font) = font else {
            context.warning(format!("{name}(): Could not find/open font"));
            return Ok(Value::Bool(false));
        };
        let glyphs = blocking(move || {
            let file = std::fs::File::open(font).map_err(|_| "Could not find/open font")?;
            let mut bytes = Vec::new();
            file.take(FONT_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Could not read font")?;
            if bytes.len() as u64 > FONT_BYTES || text.len() > 1_048_576 {
                return Err("Text or font exceeds the rendering limit");
            }
            Layout::new(&bytes, &text, size, angle, line_space)
        })
        .await?;
        let layout = match glyphs {
            Ok(layout) => layout,
            Err(message) => {
                context.warning(format!("{name}(): {message}"));
                return Ok(Value::Bool(false));
            }
        };
        let mut origin = (0, 0);
        if draw {
            origin = (
                i64::from(args::int(&values, 3).unwrap_or(0) as i32),
                i64::from(args::int(&values, 4).unwrap_or(0) as i32),
            );
            let color = args::int(&values, 5).unwrap_or(0) as i32;
            with_mut(&values, 0, |image| {
                layout.draw(image, origin, color);
                Ok(())
            })?;
        }
        layout.bbox(origin)
    })
}

struct Layout {
    glyphs: Vec<(Path, Point)>,
    angle: f32,
    bounds: [f32; 4],
}

impl Layout {
    fn new(
        font: &[u8],
        text: &str,
        size: f32,
        angle: f32,
        line_spacing: f32,
    ) -> Result<Self, &'static str> {
        if !size.is_finite()
            || !(0.0..=2048.0).contains(&size)
            || !angle.is_finite()
            || !line_spacing.is_finite()
        {
            return Err("Text layout is outside the rendering limit");
        }
        let font = FontRef::new(font)
            .or_else(|_| FontRef::from_index(font, 0))
            .map_err(|_| "Could not read font")?;
        let size = Size::new(size * (96.0 / 72.0));
        let location = LocationRef::default();
        let glyph_metrics = font.glyph_metrics(size, location);
        let metrics = font.metrics(size, location);
        let line_height = metrics.ascent - metrics.descent + metrics.leading;
        let charmap = font.charmap();
        let outlines = font.outline_glyphs();
        let mut pen = point(0.0, 0.0);
        let mut bounds = [0f32; 4];
        let mut glyphs = Vec::new();
        let mut segments = 0;
        for character in text.chars().take_while(|character| *character != '\0') {
            if character == '\r' {
                continue;
            }
            if character == '\n' {
                pen.x = 0.0;
                pen.y += line_height * line_spacing;
                continue;
            }
            if !pen.x.is_finite()
                || !pen.y.is_finite()
                || pen.x.abs() > 1_048_576.0
                || pen.y.abs() > 1_048_576.0
            {
                return Err("Text layout is outside the rendering limit");
            }
            let id = charmap.map(character).unwrap_or_default();
            if let Some(glyph) = outlines.get(id) {
                let mut path = Path::new();
                glyph
                    .draw(DrawSettings::unhinted(size, location), &mut path)
                    .map_err(|_| "Could not read glyph outline")?;
                if let Some([left, top, right, bottom]) = path.bounds()? {
                    segments += path.len();
                    if segments > 1_048_576 || glyphs.len() >= 16_384 {
                        return Err("Text layout is outside the rendering limit");
                    }
                    bounds[0] = bounds[0].min(left + pen.x);
                    bounds[1] = bounds[1].min(top + pen.y);
                    bounds[2] = bounds[2].max(right + pen.x);
                    bounds[3] = bounds[3].max(bottom + pen.y);
                    glyphs.push((path, pen));
                }
            }
            pen.x += glyph_metrics.advance_width(id).unwrap_or_default();
            bounds[2] = bounds[2].max(pen.x);
        }
        Ok(Self {
            glyphs,
            angle: angle.to_radians(),
            bounds,
        })
    }

    fn rotate(&self, x: f32, y: f32) -> Point {
        let (sin, cos) = self.angle.sin_cos();
        point(x * cos + y * sin, -x * sin + y * cos)
    }

    fn bbox(&self, origin: (i64, i64)) -> NativeResult {
        let [left, top, right, bottom] = self.bounds;
        let mut result = PhpArray::new();
        for (x, y) in [(left, bottom), (right, bottom), (right, top), (left, top)] {
            let p = self.rotate(x, y);
            result.push(Value::Int(origin.0.saturating_add(p.x as i64)))?;
            result.push(Value::Int(origin.1.saturating_add(p.y as i64)))?;
        }
        Ok(Value::from(result))
    }

    fn draw(&self, image: &mut Image, origin: (i64, i64), color: i32) {
        let glyph_color = image.color(color.saturating_abs()).unwrap_or(Color::BLACK);
        let [r, g, b, a] = glyph_color.components();
        for (path, offset) in &self.glyphs {
            path.draw(self.angle, *offset, |x, y, coverage| {
                if coverage <= 0.0 || (color < 0 && coverage < 0.5) {
                    return;
                }
                let opacity = f32::from(127 - a) * coverage.clamp(0.0, 1.0);
                let pixel = Color::gd(r, g, b, 127 - opacity.round() as u8);
                let index = image.resolve(pixel);
                image.set(origin.0 + x, origin.1 + y, index);
            });
        }
    }
}
