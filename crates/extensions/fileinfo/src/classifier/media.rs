//! Detect uploaded audio and video, for example a `video/mp4` ISO media file.

use super::{Kind, be32, le16, le32};

pub(super) fn detect(bytes: &[u8]) -> Option<Kind> {
    if bytes.get(4..8) == Some(b"ftyp") {
        return iso_media(bytes);
    }
    if bytes.starts_with(b"RIFF") {
        return match bytes.get(8..12) {
            Some(b"AVI ") => Some(Kind::new(
                "video/x-msvideo",
                "avi/divx",
                "RIFF (little-endian) data, AVI",
            )),
            Some(b"WAVE") => Some(wav(bytes)),
            _ => None,
        };
    }
    if bytes.starts_with(b"OggS") {
        let header = bytes.get(..128).unwrap_or(bytes);
        let codec = if contains(header, b"OpusHead") {
            "Opus audio"
        } else if contains(header, b"vorbis") {
            "Vorbis audio"
        } else if contains(header, b"FLAC") {
            "FLAC audio"
        } else if contains(header, b"theora") {
            return Some(Kind::new("video/ogg", "ogv", "Ogg data, Theora video"));
        } else {
            ""
        };
        let description = if codec.is_empty() {
            "Ogg data".into()
        } else {
            format!("Ogg data, {codec}")
        };
        return Some(Kind::new("audio/ogg", "???", description));
    }
    if bytes.starts_with(b"fLaC") {
        let description = flac(bytes).unwrap_or_else(|| "FLAC audio bitstream data".into());
        return Some(Kind::new("audio/flac", "???", description));
    }
    if let Some(description) = mp3(bytes) {
        return Some(Kind::new("audio/mpeg", "???", description));
    }
    if bytes.starts_with(b"\x1a\x45\xdf\xa3") {
        let header = bytes.get(..512).unwrap_or(bytes);
        if contains(header, b"webm") {
            return Some(Kind::new("video/webm", "???", "WebM"));
        }
        if contains(header, b"matroska") {
            return Some(Kind::new("video/x-matroska", "???", "Matroska data"));
        }
    }
    None
}

fn contains(bytes: &[u8], pattern: &[u8]) -> bool {
    bytes.windows(pattern.len()).any(|window| window == pattern)
}

fn iso_media(bytes: &[u8]) -> Option<Kind> {
    let size = be32(bytes, 0)? as usize;
    let brands = bytes.get(16..size.min(bytes.len()))?;
    let primary = bytes.get(8..12)?;
    let compatible = |brand: &[u8]| {
        primary == brand
            || brands
                .as_chunks::<4>()
                .0
                .iter()
                .any(|candidate| candidate == brand)
    };
    let (mime, extension, platform) = if compatible(b"avif") {
        ("image/avif", "???", "AVIF Image")
    } else if compatible(b"avis") {
        ("image/avif", "???", "AVIF Image Sequence")
    } else if compatible(b"heic") {
        (
            "image/heic",
            "???",
            "HEIF Image HEVC Main or Main Still Picture Profile",
        )
    } else if compatible(b"heix") {
        ("image/heic", "???", "HEIF Image HEVC Main 10 Profile")
    } else if compatible(b"qt  ") {
        (
            "video/quicktime",
            "???",
            "Apple QuickTime movie, Apple QuickTime (.MOV/QT)",
        )
    } else if compatible(b"M4A ") {
        (
            "audio/x-m4a",
            "???",
            "Apple iTunes ALAC/AAC-LC (.M4A) Audio",
        )
    } else if compatible(b"mif1") {
        ("image/heif", "???", "HEIF Image")
    } else {
        match primary {
            b"mp42" => ("video/mp4", "mp4", "MP4 v2 [ISO 14496-14]"),
            b"mp41" => ("video/mp4", "mp4", "MP4 v1 [ISO 14496-1:ch13]"),
            _ => ("video/mp4", "mp4", "MP4 Base Media v1 [ISO 14496-12:2003]"),
        }
    };
    Some(Kind::new(mime, extension, format!("ISO Media, {platform}")))
}

fn wav(bytes: &[u8]) -> Kind {
    let mut description = String::from("RIFF (little-endian) data, WAVE audio");
    if bytes.get(12..16) == Some(b"fmt ") {
        let method = match le16(bytes, 20) {
            Some(1) => "Microsoft PCM",
            Some(3) => "IEEE Float",
            _ => "unknown encoding",
        };
        let channels = match le16(bytes, 22) {
            Some(1) => "mono".into(),
            Some(2) => "stereo".into(),
            Some(count) => format!("{count} channels"),
            None => return Kind::new("audio/x-wav", "wav/wave", description),
        };
        let rate = le32(bytes, 24).unwrap_or_default();
        let depth = le16(bytes, 34).unwrap_or_default();
        description.push_str(&format!(", {method}, {depth} bit, {channels} {rate} Hz"));
    }
    Kind::new("audio/x-wav", "wav/wave", description)
}

fn flac(bytes: &[u8]) -> Option<String> {
    let details = u64::from_be_bytes(bytes.get(18..26)?.try_into().ok()?);
    let depth = ((details >> 36) & 0x1f) + 1;
    let channels = match (details >> 41) & 7 {
        0 => "mono".into(),
        1 => "stereo".into(),
        count => format!("{} channels", count + 1),
    };
    let samples = details & 0x0f_ffff_ffff;
    Some(format!(
        "FLAC audio bitstream data, {depth} bit, {channels}, {samples} samples"
    ))
}

fn mp3(bytes: &[u8]) -> Option<String> {
    if let [b'I', b'D', b'3', major, minor, _, a, b, c, d, ..] = bytes {
        let size = [a, b, c, d]
            .into_iter()
            .fold(0usize, |size, byte| (size << 7) | usize::from(byte & 0x7f));
        let mut description = format!("Audio file with ID3 version 2.{major}.{minor}");
        if let Some(stream) = bytes.get(10 + size..).and_then(mp3_frame) {
            description.push_str(&format!(", contains:\n- {stream}"));
        }
        return Some(description);
    }
    mp3_frame(bytes)
}

fn mp3_frame(bytes: &[u8]) -> Option<String> {
    let [0xff, header, rates, mode, ..] = bytes else {
        return None;
    };
    if header & 0xe6 != 0xe2 {
        return None;
    }
    let (version, sample_rates, bit_rates) = match (header >> 3) & 3 {
        0 => (
            "2.5",
            [11, 12, 8],
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
        ),
        2 => (
            "2",
            [22, 24, 16],
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160],
        ),
        3 => (
            "1",
            [44, 48, 32],
            [
                0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
            ],
        ),
        _ => return None,
    };
    let bitrate = bit_rates.get((rates >> 4) as usize)?;
    if *bitrate == 0 {
        return None;
    }
    let frequency = sample_rates.get(((rates >> 2) & 3) as usize)?;
    let channels = match mode >> 6 {
        0 => "Stereo",
        1 => "JntStereo",
        2 => "Dual Channel",
        _ => "Monaural",
    };
    Some(format!(
        "MPEG ADTS, layer III,  v{version},  {bitrate} kbps, {frequency} kHz, {channels}"
    ))
}
