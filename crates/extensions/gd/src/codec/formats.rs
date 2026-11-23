//! `imagecreatefromstring($bytes)` chooses a codec by signature.

use phera_extension_api::{NativeError, Value, args};

use crate::core::value_error;
use crate::pixels::Image;

use super::{gif, png, raster};

#[derive(Clone, Copy)]
pub(super) enum Format {
    Png,
    Gif,
    Jpeg,
    Bmp,
    Webp,
}

pub(super) struct Options {
    pub quality: i64,
    pub filters: i64,
    pub bmp_compressed: bool,
}

impl Options {
    pub fn from_arguments(values: &[Value], format: Format) -> Result<Self, NativeError> {
        let quality = args::int(values, 2).unwrap_or(-1);
        let invalid = match format {
            Format::Png if !(-1..=9).contains(&quality) => Some("must be between -1 and 9"),
            Format::Jpeg if !(-1..=100).contains(&quality) => Some("must be at between -1 and 100"),
            Format::Webp if quality < -1 => Some("must be greater than or equal to -1"),
            _ => None,
        };
        if let Some(message) = invalid {
            return Err(value_error(format.writer(), 3, "quality", message));
        }
        Ok(Self {
            quality,
            filters: args::int(values, 3).unwrap_or(-1),
            bmp_compressed: args::bool(values, 2).unwrap_or(true),
        })
    }
}

impl Format {
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 12 {
            return None;
        }
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(b"GIF") {
            Some(Self::Gif)
        } else if bytes.starts_with(b"\xff\xd8\xff") {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"BM") {
            Some(Self::Bmp)
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            Some(Self::Webp)
        } else {
            None
        }
    }

    pub fn reader(self) -> &'static str {
        match self {
            Self::Png => "imagecreatefrompng",
            Self::Gif => "imagecreatefromgif",
            Self::Jpeg => "imagecreatefromjpeg",
            Self::Bmp => "imagecreatefrombmp",
            Self::Webp => "imagecreatefromwebp",
        }
    }

    pub fn writer(self) -> &'static str {
        match self {
            Self::Png => "imagepng",
            Self::Gif => "imagegif",
            Self::Jpeg => "imagejpeg",
            Self::Bmp => "imagebmp",
            Self::Webp => "imagewebp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Gif => "GIF",
            Self::Jpeg => "JPEG",
            Self::Bmp => "BMP",
            Self::Webp => "WEBP",
        }
    }

    pub fn decode(self, data: &[u8]) -> Result<Image, String> {
        match self {
            Self::Png => png::decode(data),
            Self::Gif => gif::decode(data),
            _ => raster::decode(data, self),
        }
    }

    pub fn encode(self, image: &Image, options: Options) -> Result<Vec<u8>, String> {
        match self {
            Self::Png => png::encode(image, options),
            Self::Gif => gif::encode(image),
            _ => raster::encode(image, self, options),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Format;

    #[test]
    fn truncated_and_damaged_uploads_do_not_panic() {
        for (format, original) in [
            (
                Format::Png,
                include_bytes!("../../tests/corpus/rgba.png").as_slice(),
            ),
            (
                Format::Gif,
                include_bytes!("../../tests/corpus/indexed.gif").as_slice(),
            ),
            (
                Format::Jpeg,
                include_bytes!("../../tests/corpus/gradient.jpeg").as_slice(),
            ),
            (
                Format::Bmp,
                include_bytes!("../../tests/corpus/rgb.bmp").as_slice(),
            ),
            (
                Format::Bmp,
                include_bytes!("../../tests/corpus/indexed.bmp").as_slice(),
            ),
            (
                Format::Webp,
                include_bytes!("../../tests/corpus/rgba-lossy.webp").as_slice(),
            ),
        ] {
            for end in 0..original.len() {
                let _ = format.decode(&original[..end]);
            }
            let mut corrupt = original.to_vec();
            for index in 0..original.len() {
                corrupt[index] = !original[index];
                let _ = format.decode(&corrupt);
                corrupt[index] = original[index];
            }
        }
    }
}
