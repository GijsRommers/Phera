//! Parse dimensions for PHP's image metadata functions without decoding pixels.

mod raster;
mod text;

use phera_extension_api::{ArrayKey, PhpArray, Value};

use super::Kind;
pub(super) use text::{svg, xbm};

pub struct Size {
    pub width: u32,
    pub height: u32,
    pub bits: u16,
    pub channels: u16,
    pub width_unit: &'static str,
    pub height_unit: &'static str,
}

impl Size {
    fn new(width: u32, height: u32, bits: u16, channels: u16) -> Self {
        Self {
            width,
            height,
            bits,
            channels,
            width_unit: "px",
            height_unit: "px",
        }
    }
}

pub(super) fn read(kind: Kind, bytes: &[u8], info: &mut PhpArray) -> Option<Size> {
    match kind {
        Kind::Gif => Some(Size::new(
            le16(bytes, 6)?.into(),
            le16(bytes, 8)?.into(),
            u16::from(bytes.get(10)? & 7) + 1,
            3,
        )),
        Kind::Jpeg => jpeg(bytes, info),
        Kind::Png => Some(Size::new(
            be32(bytes, 16)?,
            be32(bytes, 20)?,
            (*bytes.get(24)?).into(),
            0,
        )),
        Kind::Bmp => raster::bmp(bytes),
        Kind::Ico => raster::ico(bytes),
        Kind::Webp => raster::webp(bytes),
        Kind::TiffLe | Kind::TiffBe => raster::tiff(bytes),
        Kind::Psd => Some(Size::new(
            be32(bytes, 18)?,
            be32(bytes, 14)?,
            be16(bytes, 22)?,
            be16(bytes, 12)?,
        )),
        Kind::Avif | Kind::Heif => raster::ispe(bytes),
        Kind::Wbmp => wbmp(bytes),
        Kind::Xbm => xbm(bytes),
        Kind::Svg => svg(bytes),
        _ => None,
    }
}

pub(super) fn wbmp(bytes: &[u8]) -> Option<Size> {
    let rest = bytes.strip_prefix(b"\0\0")?;
    let (width, rest) = multibyte(rest)?;
    let (height, _) = multibyte(rest)?;
    if width == 0 || height == 0 || width > 2048 || height > 2048 {
        return None;
    }
    Some(Size::new(width, height, 0, 0))
}

fn multibyte(mut rest: &[u8]) -> Option<(u32, &[u8])> {
    let mut result = 0;
    for _ in 0..4 {
        let (&value, tail) = rest.split_first()?;
        result = (result << 7) | u32::from(value & 127);
        rest = tail;
        if value < 128 {
            return Some((result, rest));
        }
    }
    None
}

fn jpeg(bytes: &[u8], info: &mut PhpArray) -> Option<Size> {
    let mut bytes = bytes.get(2..)?;
    let mut size = None;
    while let Some(start) = bytes.iter().position(|&byte| byte == 0xff) {
        bytes = bytes.get(start..)?;
        while let Some(rest) = bytes.strip_prefix(&[0xff]) {
            bytes = rest;
        }
        let (&marker, rest) = bytes.split_first()?;
        if matches!(marker, 0xda | 0xd9) {
            break;
        }
        if matches!(marker, 0x00 | 0x01 | 0xd0..=0xd7) {
            bytes = rest;
            continue;
        }
        let length = usize::from(be16(rest, 0)?);
        let Some(data) = rest.get(2..length) else {
            break;
        };
        if matches!(marker, 0xe0..=0xef) {
            let name = ArrayKey::str(format!("APP{}", marker - 0xe0));
            if info.get(&name).is_none() {
                info.insert(name, Value::from(data.to_vec()));
            }
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf)
            && size.is_none()
            && let Some(info) = data.get(..6)
        {
            size = Some(Size::new(
                u32::from(u16::from_be_bytes([info[3], info[4]])),
                u32::from(u16::from_be_bytes([info[1], info[2]])),
                u16::from(info[0]),
                u16::from(info[5]),
            ));
        }
        bytes = &rest[length..];
    }
    size
}

fn be16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..)?.get(..2)?.try_into().ok()?,
    ))
}

fn le16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        data.get(offset..)?.get(..2)?.try_into().ok()?,
    ))
}

fn be32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..)?.get(..4)?.try_into().ok()?,
    ))
}

fn le32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        data.get(offset..)?.get(..4)?.try_into().ok()?,
    ))
}
