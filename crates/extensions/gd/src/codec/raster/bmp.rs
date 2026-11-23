//! GD reads indexed BMP colors and uses RLE8 for compressed palette output.

mod indexed;

use std::io::{BufRead, Seek};

use image::codecs::bmp::{BmpDecoder, BmpEncoder};
use image::{DynamicImage, ExtendedColorType, ImageDecoder};

use crate::pixels::{Color, Image, Storage, pixel_count};

pub(super) fn decode<R: BufRead + Seek>(
    decoder: BmpDecoder<R>,
    data: &[u8],
) -> Result<Image, String> {
    let (width, height) = decoder.dimensions();
    let count = pixel_count(width, height).ok_or("BMP dimensions exceed the pixel limit")?;
    let storage = if let Some(palette) = decoder.get_palette() {
        let palette = palette
            .iter()
            .map(|&[r, g, b]| Color::gd(r, g, b, 0))
            .collect();
        let pixels = indexed::read(data, width, height, count)?;
        Storage::Indexed { pixels, palette }
    } else {
        let image = DynamicImage::from_decoder(decoder).map_err(super::to_string)?;
        let pixels = image
            .into_rgba8()
            .pixels()
            .map(|pixel| Color::from_rgba(pixel.0))
            .collect();
        Storage::TrueColor(pixels)
    };
    let mut image = Image::with_storage(width, height, storage);
    if let Some(dpi) = read_resolution(data) {
        image.resolution = dpi;
    }
    Ok(image)
}

fn read_resolution(data: &[u8]) -> Option<[u32; 2]> {
    let size = u32::from_le_bytes(data.get(14..18)?.try_into().ok()?);
    if size < 40 {
        return None;
    }
    let x = u32::from_le_bytes(data.get(38..42)?.try_into().ok()?);
    let y = u32::from_le_bytes(data.get(42..46)?.try_into().ok()?);
    if x == 0 || y == 0 {
        return None;
    }
    Some([x, y].map(|dots| (f64::from(dots) * 0.0254).round() as u32))
}

pub(super) fn encode(image: &Image, compressed: bool) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut encoder = BmpEncoder::new(&mut output);
    match &image.storage {
        Storage::Indexed { pixels, palette } => {
            let palette: Vec<_> = palette
                .iter()
                .map(|color| {
                    let [r, g, b, _] = color.components();
                    [r, g, b]
                })
                .collect();
            if compressed && !palette.is_empty() {
                return Ok(encode_rle(image, pixels, &palette));
            }
            encoder
                .encode_with_palette(
                    pixels,
                    image.width,
                    image.height,
                    ExtendedColorType::L8,
                    Some(&palette),
                )
                .map_err(super::to_string)?;
        }
        Storage::TrueColor(_) => {
            let rgb = super::rgb(image);
            encoder
                .encode(&rgb, image.width, image.height, ExtendedColorType::Rgb8)
                .map_err(super::to_string)?;
        }
    }
    Ok(output)
}

fn encode_rle(image: &Image, pixels: &[u8], palette: &[[u8; 3]]) -> Vec<u8> {
    let offset = 54 + 4 * palette.len() as u32;
    let mut output = Vec::new();
    output.extend_from_slice(b"BM");
    for value in [0, 0, offset, 40, image.width, image.height] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    output.extend_from_slice(&1u16.to_le_bytes());
    output.extend_from_slice(&8u16.to_le_bytes());
    for value in [1, 0, 0, 0, palette.len() as u32, 0] {
        output.extend_from_slice(&value.to_le_bytes());
    }
    for &[r, g, b] in palette {
        output.extend_from_slice(&[b, g, r, 0]);
    }
    for row in pixels.chunks_exact(image.width as usize).rev() {
        let mut tail = row;
        while let Some(&color) = tail.first() {
            let run = tail
                .iter()
                .take(255)
                .take_while(|&&pixel| pixel == color)
                .count();
            output.extend_from_slice(&[run as u8, color]);
            tail = &tail[run..];
        }
        output.extend_from_slice(&[0, 0]);
    }
    output.extend_from_slice(&[0, 1]);
    let size = output.len() as u32;
    output[2..6].copy_from_slice(&size.to_le_bytes());
    output[34..38].copy_from_slice(&(size - offset).to_le_bytes());
    output
}
