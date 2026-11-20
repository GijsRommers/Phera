//! `imagecreatefrompng` preserves palette indices and PNG alpha as GD colors.

use std::io::Cursor;

use ::png::{BitDepth, ColorType, DeflateCompression, Filter, Limits, PixelDimensions, Unit};

use crate::pixels::{Color, Image, Storage, pixel_count};

use super::Options;

pub(super) fn decode(data: &[u8]) -> Result<Image, String> {
    let mut decoder = ::png::Decoder::new_with_limits(
        Cursor::new(data),
        Limits {
            bytes: 64 * 1024 * 1024,
        },
    );
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    decoder.set_transformations(::png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(to_string)?;
    let info = reader.info();
    let count = pixel_count(info.width, info.height).ok_or_else(too_large)?;
    let interlaced = info.interlaced;
    let resolution = info
        .pixel_dims
        .filter(|value| value.unit == Unit::Meter)
        .map(|value| {
            [value.xppu, value.yppu].map(|dots| (f64::from(dots) * 0.0254).round() as u32)
        });
    let trns = info.trns.as_deref().unwrap_or_default().to_vec();
    let palette = info.palette.as_deref().unwrap_or_default().to_vec();
    let mut bytes = vec![0; reader.output_buffer_size().ok_or_else(too_large)?];
    let frame = reader.next_frame(&mut bytes).map_err(to_string)?;
    reader.finish().map_err(to_string)?;
    let width = frame.width;
    let height = frame.height;
    bytes.truncate(frame.buffer_size());
    let (storage, transparent) = match frame.color_type {
        ColorType::Grayscale | ColorType::Indexed => {
            let (palette, transparent) =
                indexed_palette(frame.color_type, frame.bit_depth, &palette, &trns);
            let pixels = unpack(&bytes, width, frame.bit_depth, frame.line_size, count);
            (Storage::Indexed { pixels, palette }, transparent)
        }
        ColorType::Rgb | ColorType::Rgba | ColorType::GrayscaleAlpha => {
            let pixels = bytes
                .chunks_exact(frame.color_type.samples())
                .filter_map(|pixel| match *pixel {
                    [r, g, b] => Some(Color::gd(r, g, b, 0)),
                    [r, g, b, a] => Some(Color::from_rgba([r, g, b, a])),
                    [gray, a] => Some(Color::from_rgba([gray, gray, gray, a])),
                    _ => None,
                })
                .collect();
            let transparent = match trns.as_slice() {
                [r, g, b] => Some(Color::gd(*r, *g, *b, 0).0 as i32),
                [_, r, _, g, _, b] => Some(Color::gd(*r, *g, *b, 0).0 as i32),
                _ => None,
            };
            (Storage::TrueColor(pixels), transparent)
        }
    };
    let mut image = Image::with_storage(width, height, storage);
    image.interlaced = interlaced;
    image.transparent = transparent;
    if let Some(resolution) = resolution {
        image.resolution = resolution;
    }
    Ok(image)
}

fn too_large() -> String {
    "Image dimensions exceed the pixel limit".into()
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn indexed_palette(
    kind: ColorType,
    depth: BitDepth,
    palette: &[u8],
    trns: &[u8],
) -> (Vec<Color>, Option<i32>) {
    if matches!(kind, ColorType::Indexed) {
        let entries = palette
            .as_chunks::<3>()
            .0
            .iter()
            .enumerate()
            .map(|(index, rgb)| {
                let alpha = trns.get(index).copied().unwrap_or(255);
                Color::from_rgba([rgb[0], rgb[1], rgb[2], alpha])
            })
            .collect();
        let transparent = trns
            .iter()
            .position(|alpha| *alpha == 0)
            .map(|index| index as i32);
        return (entries, transparent);
    }
    let levels = 1usize << depth as usize;
    let palette = (0..levels)
        .map(|index| {
            let gray = (255 * index / (levels - 1)) as u8;
            Color::gd(gray, gray, gray, 0)
        })
        .collect();
    let transparent = match trns {
        [value] | [_, value] => Some(i32::from(*value)),
        _ => None,
    };
    (palette, transparent)
}

fn unpack(bytes: &[u8], width: u32, depth: BitDepth, stride: usize, count: usize) -> Vec<u8> {
    if depth == BitDepth::Eight {
        return bytes.to_vec();
    }
    let depth = depth as usize;
    let mask = (1u8 << depth) - 1;
    let mut pixels = Vec::with_capacity(count);
    for row in bytes.chunks_exact(stride) {
        for x in 0..width as usize {
            let offset = x * depth;
            pixels.push((row[offset / 8] >> (8 - depth - offset % 8)) & mask);
        }
    }
    pixels
}

pub(super) fn encode(image: &Image, options: Options) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut encoder = ::png::Encoder::new(&mut result, image.width, image.height);
    encoder.set_depth(BitDepth::Eight);
    encoder.set_deflate_compression(match options.quality {
        0 => DeflateCompression::NoCompression,
        1..=9 => DeflateCompression::Level(options.quality as u8),
        _ => DeflateCompression::Level(6),
    });
    encoder.set_filter(match options.filters {
        0 | 8 => Filter::NoFilter,
        16 => Filter::Sub,
        32 => Filter::Up,
        64 => Filter::Avg,
        128 => Filter::Paeth,
        _ => Filter::Adaptive,
    });
    encoder.set_pixel_dims(Some(PixelDimensions {
        xppu: (f64::from(image.resolution[0]) / 0.0254).round() as u32,
        yppu: (f64::from(image.resolution[1]) / 0.0254).round() as u32,
        unit: Unit::Meter,
    }));
    let owned;
    let raw = match &image.storage {
        Storage::Indexed { pixels, palette } => {
            if palette.is_empty() {
                return Err("PNG image has no palette".into());
            }
            encoder.set_color(ColorType::Indexed);
            encoder.set_palette(
                palette
                    .iter()
                    .flat_map(|color| {
                        let [r, g, b, _] = color.components();
                        [r, g, b]
                    })
                    .collect::<Vec<_>>(),
            );
            let transparency: Vec<_> = palette
                .iter()
                .enumerate()
                .map(|(index, color)| {
                    if image.transparent == Some(index as i32) {
                        0
                    } else {
                        color.rgba(true)[3]
                    }
                })
                .collect();
            if transparency.iter().any(|value| *value != 255) {
                encoder.set_trns(transparency);
            }
            pixels
        }
        Storage::TrueColor(pixels) => {
            owned = if image.save_alpha {
                encoder.set_color(ColorType::Rgba);
                pixels.iter().flat_map(|color| color.rgba(true)).collect()
            } else {
                encoder.set_color(ColorType::Rgb);
                if let Some(transparent) = image.transparent {
                    let [red, green, blue, _] = Color(transparent as u32).components();
                    encoder.set_trns(vec![0, red, 0, green, 0, blue]);
                }
                pixels
                    .iter()
                    .flat_map(|color| {
                        let [r, g, b, _] = color.components();
                        [r, g, b]
                    })
                    .collect()
            };
            &owned
        }
    };
    let mut writer = encoder.write_header().map_err(to_string)?;
    writer.write_image_data(raw).map_err(to_string)?;
    writer.finish().map_err(to_string)?;
    Ok(result)
}
