//! Probe content rather than extensions, as in `(new finfo(FILEINFO_MIME_TYPE))->buffer($bytes)`.

mod archive;
mod binary;
mod image;
mod media;
mod text;

pub const FLAGS: &[(&str, i64)] = &[
    ("FILEINFO_NONE", 0),
    ("FILEINFO_SYMLINK", 2),
    ("FILEINFO_DEVICES", 8),
    ("FILEINFO_MIME_TYPE", 16),
    ("FILEINFO_CONTINUE", 32),
    ("FILEINFO_PRESERVE_ATIME", 128),
    ("FILEINFO_RAW", 256),
    ("FILEINFO_MIME_ENCODING", 1024),
    ("FILEINFO_MIME", 1040),
    ("FILEINFO_APPLE", 2048),
    ("FILEINFO_EXTENSION", 16_777_216),
];

#[derive(Clone, Copy)]
pub enum Format {
    Description,
    RawDescription,
    MimeType,
    Encoding,
    Mime,
    Extension,
    Apple,
    DisabledMime,
}

impl Format {
    pub fn new(flags: i32) -> Self {
        if flags == -1 {
            return Self::DisabledMime;
        }
        match (flags & 16 != 0, flags & 1024 != 0) {
            (true, true) => Self::Mime,
            (true, false) => Self::MimeType,
            (false, true) => Self::Encoding,
            _ if flags & 16_777_216 != 0 => Self::Extension,
            _ if flags & 2048 != 0 => Self::Apple,
            _ if flags & 256 != 0 => Self::RawDescription,
            _ => Self::Description,
        }
    }
}

struct Kind {
    mime: &'static str,
    extensions: &'static str,
    description: String,
    apple: &'static str,
}

impl Kind {
    fn new(mime: &'static str, extensions: &'static str, description: impl Into<String>) -> Self {
        Self {
            mime,
            extensions,
            description: description.into(),
            apple: "UNKNUNKN",
        }
    }

    fn apple(mut self, code: &'static str) -> Self {
        self.apple = code;
        self
    }
}

pub fn identify(bytes: &[u8], tail: &[u8], format: Format) -> String {
    let encoding = text::Encoding::detect(bytes.get(..65_536).unwrap_or(bytes));
    if matches!(format, Format::Encoding) {
        return encoding.name().into();
    }
    let kind = match bytes.len() {
        0 => Kind::new("application/x-empty", "???", "empty"),
        1 => Kind::new(
            "application/octet-stream",
            "???",
            "very short file (no magic)",
        ),
        _ => image::detect(bytes)
            .or_else(|| archive::detect(bytes, tail))
            .or_else(|| media::detect(bytes))
            .or_else(|| binary::detect(bytes))
            .or_else(|| text::detect(bytes, encoding))
            .unwrap_or_else(|| Kind::new("application/octet-stream", "???", "data")),
    };
    let mime = kind.mime;
    match format {
        Format::Description => escaped(&kind.description),
        Format::RawDescription => kind.description,
        Format::MimeType => mime.into(),
        Format::Mime => format!("{mime}; charset={}", encoding.name()),
        Format::Extension => kind.extensions.into(),
        Format::Apple => kind.apple.into(),
        Format::DisabledMime => "application/octet-stream; charset=binary".into(),
        Format::Encoding => encoding.name().into(),
    }
}

fn escaped(description: &str) -> String {
    use std::fmt::Write;

    if !description.bytes().any(|byte| byte.is_ascii_control()) {
        return description.into();
    }
    let mut output = String::with_capacity(description.len());
    for character in description.chars() {
        if character.is_ascii_control() {
            let _ = write!(output, "\\{:03o}", character as u8);
        } else {
            output.push(character);
        }
    }
    output
}

fn le16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..)?.get(..2)?.try_into().ok()?,
    ))
}

fn le32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..)?.get(..4)?.try_into().ok()?,
    ))
}

fn be16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..)?.get(..2)?.try_into().ok()?,
    ))
}

fn be32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..)?.get(..4)?.try_into().ok()?,
    ))
}
