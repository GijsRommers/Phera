//! `imagettfbbox(14, 0, 'font.ttf', 'Hi')` lays out glyphs using a Rust rasterizer.

use std::io::Read;

use ab_glyph::{Font, FontVec, Point, PxScale, ScaleFont, point};
use phera_ext_streams::file_io::{blocking, resolve};
use phera_extension_api::{
    ArrayKey, ConstantValue as C, Context, FunctionEntry as F, LocalFuture, NativeResult,
    Param as P, ParamType as T, PhpArray, Registry, Value, args, value::to_float,
};

use crate::core::{IMAGE, with_mut};
use crate::pixels::{Color, Image};

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
const GLYPH_PIXELS: f32 = 1_048_576.0;

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
            let font = FontVec::try_from_vec(bytes).map_err(|_| "Could not read font")?;
            Layout::new(font, &text, size, angle, line_space)
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
    glyphs: Vec<ab_glyph::OutlinedGlyph>,
    angle: f32,
    bounds: [f32; 4],
}

impl Layout {
    fn new(
        font: FontVec,
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
        let scale = font
            .pt_to_px_scale(size)
            .unwrap_or_else(|| PxScale::from(size * 4.0 / 3.0));
        let font_scaled = font.as_scaled(scale);
        let mut pen = point(0.0, 0.0);
        let mut previous = None;
        let mut bounds = [0f32; 4];
        let mut glyphs = Vec::new();
        for character in text.chars().take_while(|character| *character != '\0') {
            if character == '\r' {
                continue;
            }
            if character == '\n' {
                pen.x = 0.0;
                pen.y += font_scaled.height() * line_spacing;
                previous = None;
                continue;
            }
            let id = font_scaled.glyph_id(character);
            if let Some(prior) = previous {
                pen.x += font_scaled.kern(prior, id);
            }
            let glyph = id.with_scale_and_position(scale, pen);
            pen.x += font_scaled.h_advance(id);
            previous = Some(id);
            if let Some(outline) = font.outline_glyph(glyph) {
                let rect = outline.px_bounds();
                let pixels = rect.width() * rect.height();
                if !pixels.is_finite()
                    || !(0.0..=GLYPH_PIXELS).contains(&pixels)
                    || glyphs.len() >= 16384
                {
                    return Err("Glyph exceeds the rendering limit");
                }
                bounds[0] = bounds[0].min(rect.min.x);
                bounds[1] = bounds[1].min(rect.min.y);
                bounds[2] = bounds[2].max(rect.max.x);
                bounds[3] = bounds[3].max(rect.max.y);
                glyphs.push(outline);
            }
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
        let right = i64::from(image.width);
        let bottom = i64::from(image.height);
        for outline in &self.glyphs {
            let rect = outline.px_bounds();
            let corners = [
                rect.min,
                rect.max,
                point(rect.min.x, rect.max.y),
                point(rect.max.x, rect.min.y),
            ]
            .map(|p| self.rotate(p.x, p.y));
            if corners
                .iter()
                .all(|p| (p.x as i64).saturating_add(origin.0) >= right)
                || corners
                    .iter()
                    .all(|p| (p.x as i64).saturating_add(origin.0) < 0)
                || corners
                    .iter()
                    .all(|p| (p.y as i64).saturating_add(origin.1) >= bottom)
                || corners
                    .iter()
                    .all(|p| (p.y as i64).saturating_add(origin.1) < 0)
            {
                continue;
            }
            outline.draw(|x, y, coverage| {
                if coverage <= 0.0 || (color < 0 && coverage < 0.5) {
                    return;
                }
                let p = self.rotate(rect.min.x + x as f32, rect.min.y + y as f32);
                let opacity = f32::from(127 - a) * coverage.clamp(0.0, 1.0);
                let pixel = Color::gd(r, g, b, 127 - opacity.round() as u8);
                let index = image.resolve(pixel);
                image.set(
                    origin.0.saturating_add(p.x.round() as i64),
                    origin.1.saturating_add(p.y.round() as i64),
                    index,
                );
            });
        }
    }
}
