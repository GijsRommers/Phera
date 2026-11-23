//! A GIF decodes into a GD palette image using the first frame's original indices.

use std::borrow::Cow;
use std::num::NonZeroU64;

use ::gif::{ColorOutput, DecodeOptions, Encoder, Frame, MemoryLimit};

use crate::pixels::{Color, Image, MAX_PIXELS, Storage, pixel_count};

pub(super) fn decode(bytes: &[u8]) -> Result<Image, String> {
    let limit = NonZeroU64::new(MAX_PIXELS as u64).ok_or("Invalid pixel limit")?;
    let mut options = DecodeOptions::new();
    options.set_memory_limit(MemoryLimit::Bytes(limit));
    options.set_color_output(ColorOutput::Indexed);
    options.check_frame_consistency(true);
    let mut reader = options
        .read_info(bytes)
        .map_err(|error| error.to_string())?;
    pixel_count(u32::from(reader.width()), u32::from(reader.height()))
        .ok_or("GIF dimensions exceed the pixel limit")?;
    let global = reader.global_palette().map(<[u8]>::to_vec);
    let frame = reader
        .read_next_frame()
        .map_err(|error| error.to_string())?
        .ok_or("GIF contains no frame")?;
    let width = u32::from(frame.width);
    let height = u32::from(frame.height);
    pixel_count(width, height).ok_or("GIF dimensions exceed the pixel limit")?;
    let palette_bytes = frame
        .palette
        .as_deref()
        .or(global.as_deref())
        .unwrap_or(&[0, 0, 0, 255, 255, 255]);
    let pixels = frame.buffer.to_vec();
    let colors = usize::from(
        pixels
            .iter()
            .copied()
            .max()
            .ok_or("GIF contains no pixels")?,
    ) + 1;
    let palette = (0..colors)
        .map(|index| {
            let alpha = if Some(index as u8) == frame.transparent {
                127
            } else {
                0
            };
            let rgb = palette_bytes
                .get(index * 3..index * 3 + 3)
                .unwrap_or(&[0; 3]);
            Color::gd(rgb[0], rgb[1], rgb[2], alpha)
        })
        .collect();
    let mut image = Image::with_storage(width, height, Storage::Indexed { pixels, palette });
    image.transparent = frame.transparent.map(i32::from);
    image.interlaced = frame.interlaced;
    Ok(image)
}

pub(super) fn encode(image: &Image) -> Result<Vec<u8>, String> {
    let width = u16::try_from(image.width).map_err(|_| "GIF width is too large")?;
    let height = u16::try_from(image.height).map_err(|_| "GIF height is too large")?;
    let mut result = Vec::new();
    match &image.storage {
        Storage::Indexed { pixels, palette } => {
            let palette: Vec<_> = palette
                .iter()
                .flat_map(|color| {
                    let [r, g, b, _] = color.components();
                    [r, g, b]
                })
                .collect();
            let mut writer = Encoder::new(&mut result, width, height, &palette)
                .map_err(|error| error.to_string())?;
            let frame = Frame {
                width,
                height,
                buffer: Cow::Borrowed(pixels),
                transparent: image.transparent.and_then(|color| u8::try_from(color).ok()),
                ..Frame::default()
            };
            writer
                .write_frame(&frame)
                .map_err(|error| error.to_string())?;
            writer.into_inner().map_err(|error| error.to_string())?;
        }
        Storage::TrueColor(pixels) => {
            let mut bytes: Vec<_> = pixels
                .iter()
                .flat_map(|color| {
                    let [r, g, b, _] = color.components();
                    let alpha = if image.transparent == Some(color.0 as i32) {
                        0
                    } else {
                        255
                    };
                    [r, g, b, alpha]
                })
                .collect();
            let frame = Frame::from_rgba(width, height, &mut bytes);
            let mut writer =
                Encoder::new(&mut result, width, height, &[]).map_err(|error| error.to_string())?;
            writer
                .write_frame(&frame)
                .map_err(|error| error.to_string())?;
            writer.into_inner().map_err(|error| error.to_string())?;
        }
    }
    Ok(result)
}
