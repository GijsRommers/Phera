//! JPEG, BMP and WebP use the image-rs codecs behind `imagecreatefromjpeg` and peers.

mod bmp;

use std::io::Cursor;

use image::codecs::{
    bmp::BmpDecoder,
    jpeg::{JpegEncoder, PixelDensity, PixelDensityUnit},
    webp::WebPEncoder,
};
use image::{DynamicImage, ExtendedColorType, ImageDecoder, ImageFormat, ImageReader};

use crate::pixels::{Color, Image, MAX_DIMENSION, Storage, pixel_count};

use super::{Format, Options};

pub(super) fn decode(data: &[u8], format: Format) -> Result<Image, String> {
    if matches!(format, Format::Bmp) {
        return bmp::decode(BmpDecoder::new(Cursor::new(data)).map_err(to_string)?, data);
    }
    let image_format = match format {
        Format::Jpeg => ImageFormat::Jpeg,
        Format::Webp => ImageFormat::WebP,
        _ => return Err("Unsupported codec".into()),
    };
    let mut reader = ImageReader::with_format(Cursor::new(data), image_format);
    reader.limits(limits());
    let decoder = reader.into_decoder().map_err(to_string)?;
    let (width, height) = decoder.dimensions();
    pixel_count(width, height).ok_or("Image dimensions exceed the pixel limit")?;
    let image = DynamicImage::from_decoder(decoder).map_err(to_string)?;
    let pixels = image
        .into_rgba8()
        .pixels()
        .map(|pixel| Color::from_rgba(pixel.0))
        .collect();
    let mut image = Image::with_storage(width, height, Storage::TrueColor(pixels));
    if matches!(format, Format::Webp) {
        image.save_alpha = true;
    } else if let Some(dpi) = jpeg_density(data) {
        image.resolution = dpi;
    }
    Ok(image)
}

fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn jpeg_density(data: &[u8]) -> Option<[u32; 2]> {
    let suffix = data.strip_prefix(b"\xff\xd8\xff\xe0")?.get(2..)?;
    let suffix = suffix.strip_prefix(b"JFIF\0")?;
    let values: [u8; 4] = suffix.get(3..7)?.try_into().ok()?;
    let [a, b, c, d] = values;
    let x = u32::from(u16::from_be_bytes([a, b]));
    let y = u32::from(u16::from_be_bytes([c, d]));
    match suffix[2] {
        1 => Some([x, y]),
        2 => Some([x, y].map(|value| (f64::from(value) * 2.54).round() as u32)),
        _ => None,
    }
}

pub(super) fn encode(image: &Image, format: Format, options: Options) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    match format {
        Format::Jpeg => {
            let rgb = rgb(image);
            let quality = if options.quality < 0 {
                75
            } else {
                options.quality as u8
            };
            let mut encoder = JpegEncoder::new_with_quality(&mut output, quality);
            let [x, y] = image
                .resolution
                .map(|dpi| dpi.min(u32::from(u16::MAX)) as u16);
            encoder.set_pixel_density(PixelDensity {
                density: (x, y),
                unit: PixelDensityUnit::Inches,
            });
            encoder
                .encode(&rgb, image.width, image.height, ExtendedColorType::Rgb8)
                .map_err(to_string)?;
        }
        Format::Webp => {
            let rgba: Vec<_> = (0..image.width as usize * image.height as usize)
                .flat_map(|position| image.true_pixel(position).rgba(true))
                .collect();
            WebPEncoder::new_lossless(&mut output)
                .encode(&rgba, image.width, image.height, ExtendedColorType::Rgba8)
                .map_err(to_string)?;
        }
        Format::Bmp => return bmp::encode(image, options.bmp_compressed),
        _ => return Err("Unsupported codec".into()),
    }
    Ok(output)
}

fn rgb(image: &Image) -> Vec<u8> {
    (0..image.width as usize * image.height as usize)
        .flat_map(|position| {
            let [r, g, b, _] = image.true_pixel(position).components();
            [r, g, b]
        })
        .collect()
}
