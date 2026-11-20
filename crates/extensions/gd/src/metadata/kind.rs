//! PHP's IMAGETYPE constants and `image_type_to_mime_type(IMAGETYPE_PNG)`.

use phera_extension_api::Registry;

#[derive(Clone, Copy)]
#[repr(i64)]
pub(super) enum Kind {
    Gif = 1,
    Jpeg,
    Png,
    Swf,
    Psd,
    Bmp,
    TiffLe,
    TiffBe,
    Jpc,
    Jp2,
    Jpx,
    Jb2,
    Swc,
    Iff,
    Wbmp,
    Xbm,
    Ico,
    Webp,
    Avif,
    Heif,
    Svg,
}

const TYPES: &[(Kind, &str, &str, &str)] = &[
    (Kind::Gif, "IMAGETYPE_GIF", ".gif", "image/gif"),
    (Kind::Jpeg, "IMAGETYPE_JPEG", ".jpeg", "image/jpeg"),
    (Kind::Png, "IMAGETYPE_PNG", ".png", "image/png"),
    (
        Kind::Swf,
        "IMAGETYPE_SWF",
        ".swf",
        "application/x-shockwave-flash",
    ),
    (Kind::Psd, "IMAGETYPE_PSD", ".psd", "image/psd"),
    (Kind::Bmp, "IMAGETYPE_BMP", ".bmp", "image/bmp"),
    (Kind::TiffLe, "IMAGETYPE_TIFF_II", ".tiff", "image/tiff"),
    (Kind::TiffBe, "IMAGETYPE_TIFF_MM", ".tiff", "image/tiff"),
    (
        Kind::Jpc,
        "IMAGETYPE_JPC",
        ".jpc",
        "application/octet-stream",
    ),
    (Kind::Jp2, "IMAGETYPE_JP2", ".jp2", "image/jp2"),
    (
        Kind::Jpx,
        "IMAGETYPE_JPX",
        ".jpx",
        "application/octet-stream",
    ),
    (
        Kind::Jb2,
        "IMAGETYPE_JB2",
        ".jb2",
        "application/octet-stream",
    ),
    (
        Kind::Swc,
        "IMAGETYPE_SWC",
        ".swf",
        "application/x-shockwave-flash",
    ),
    (Kind::Iff, "IMAGETYPE_IFF", ".iff", "image/iff"),
    (Kind::Wbmp, "IMAGETYPE_WBMP", ".bmp", "image/vnd.wap.wbmp"),
    (Kind::Xbm, "IMAGETYPE_XBM", ".xbm", "image/xbm"),
    (
        Kind::Ico,
        "IMAGETYPE_ICO",
        ".ico",
        "image/vnd.microsoft.icon",
    ),
    (Kind::Webp, "IMAGETYPE_WEBP", ".webp", "image/webp"),
    (Kind::Avif, "IMAGETYPE_AVIF", ".avif", "image/avif"),
    (Kind::Heif, "IMAGETYPE_HEIF", ".heif", "image/heif"),
    (Kind::Svg, "IMAGETYPE_SVG", ".svg", "image/svg+xml"),
];

pub(super) fn register(registry: &mut Registry) {
    for &(kind, name, _, _) in TYPES {
        registry.constant(name, kind as i64);
    }
    registry.constant("IMAGETYPE_JPEG2000", Kind::Jpc as i64);
    registry.constant("IMAGETYPE_UNKNOWN", 0);
    registry.constant("IMAGETYPE_COUNT", TYPES.len() as i64 + 1);
}

impl Kind {
    pub fn from_id(value: i64) -> Option<Self> {
        let index = usize::try_from(value).ok()?.checked_sub(1)?;
        TYPES.get(index).map(|item| item.0)
    }

    pub fn extension(self) -> &'static str {
        TYPES[self as usize - 1].2
    }

    pub fn mime(self) -> &'static str {
        TYPES[self as usize - 1].3
    }
}

pub(super) fn detect(bytes: &[u8]) -> Option<Kind> {
    use Kind::*;
    let ty = match bytes {
        [b'G', b'I', b'F', ..] => Gif,
        [0xff, 0xd8, 0xff, ..] => Jpeg,
        [0x89, b'P', b'N', b'G', 13, 10, 26, 10, ..] => Png,
        [b'F', b'W', b'S', ..] => Swf,
        [b'C', b'W', b'S', ..] => Swc,
        [b'8', b'B', b'P', ..] => Psd,
        [b'B', b'M', _, ..] => Bmp,
        [b'I', b'I', 42, 0, ..] => TiffLe,
        [b'M', b'M', 0, 42, ..] => TiffBe,
        [0xff, 0x4f, 0xff, ..] => Jpc,
        [b'F', b'O', b'R', b'M', ..] => Iff,
        [0, 0, 1, 0, ..] => Ico,
        [b'R', b'I', b'F', _, _, _, _, _, b'W', b'E', b'B', b'P', ..] => Webp,
        [0, 0, 0, 12, b'j', b'P', 32, 32, 13, 10, 0x87, 10, ..] => Jp2,
        [_, _, _, _, b'f', b't', b'y', b'p', ..] => heif(bytes)?,
        [0, 0, ..] if super::headers::wbmp(bytes).is_some() => Wbmp,
        _ if super::headers::svg(bytes).is_some() => Svg,
        _ if super::headers::xbm(bytes).is_some() => Xbm,
        _ => return None,
    };
    Some(ty)
}

fn heif(bytes: &[u8]) -> Option<Kind> {
    let end = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?) as usize;
    let primary = bytes.get(8..12)?;
    if matches!(primary, b"avif" | b"avis")
        || bytes.get(16..end.min(bytes.len())).is_some_and(|brands| {
            brands
                .chunks_exact(4)
                .any(|brand| matches!(brand, b"avif" | b"avis"))
        })
    {
        Some(Kind::Avif)
    } else if matches!(primary, b"mif1" | b"heic" | b"heix") {
        Some(Kind::Heif)
    } else {
        None
    }
}
