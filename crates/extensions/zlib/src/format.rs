//! The same DEFLATE blocks can be raw or framed by zlib or gzip headers.

use adler2::Adler32;
use flate2::Crc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    Raw,
    Zlib,
    Gzip,
}

impl Encoding {
    pub(crate) fn from_php(value: i64) -> Option<Self> {
        Some(match value {
            -15 => Self::Raw,
            15 => Self::Zlib,
            31 => Self::Gzip,
            _ => return None,
        })
    }

    pub(crate) fn php(self) -> i64 {
        match self {
            Self::Raw => -15,
            Self::Zlib => 15,
            Self::Gzip => 31,
        }
    }

    pub(crate) fn header(
        self,
        out: &mut Vec<u8>,
        level: u32,
        window: u8,
        dictionary: Option<&[u8]>,
    ) {
        match self {
            Self::Raw => {}
            Self::Gzip => {
                let xfl = if level <= 1 {
                    4
                } else if level == 9 {
                    2
                } else {
                    0
                };
                let os = if cfg!(target_os = "macos") { 19 } else { 3 };
                out.extend([0x1f, 0x8b, 8, 0, 0, 0, 0, 0, xfl, os]);
            }
            Self::Zlib => {
                let cmf = ((window.max(9) - 8) << 4) | 8;
                let mut flags = match level {
                    0 | 1 => 0,
                    2..=5 => 64,
                    6 => 128,
                    _ => 192,
                };
                if dictionary.is_some() {
                    flags |= 32;
                }
                let header = u16::from_be_bytes([cmf, flags]);
                flags += ((31 - header % 31) % 31) as u8;
                out.extend([cmf, flags]);
                if let Some(dictionary) = dictionary {
                    out.extend(adler32(dictionary).to_be_bytes());
                }
            }
        }
    }
}

pub(crate) fn adler32(bytes: &[u8]) -> u32 {
    let mut checksum = Adler32::new();
    checksum.write_slice(bytes);
    checksum.checksum()
}

pub(crate) enum Checksum {
    None,
    Adler(Adler32),
    Crc(Crc),
}

impl Checksum {
    pub(crate) fn new(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Raw => Self::None,
            Encoding::Zlib => Self::Adler(Adler32::new()),
            Encoding::Gzip => Self::Crc(Crc::new()),
        }
    }

    pub(crate) fn update(&mut self, bytes: &[u8]) {
        match self {
            Self::None => {}
            Self::Adler(checksum) => checksum.write_slice(bytes),
            Self::Crc(checksum) => checksum.update(bytes),
        }
    }

    pub(crate) fn trailer(&self) -> ([u8; 8], usize) {
        let mut trailer = [0; 8];
        let length = match self {
            Self::None => 0,
            Self::Adler(adler) => {
                trailer[..4].copy_from_slice(&adler.checksum().to_be_bytes());
                4
            }
            Self::Crc(crc) => {
                trailer[..4].copy_from_slice(&crc.sum().to_le_bytes());
                trailer[4..].copy_from_slice(&crc.amount().to_le_bytes());
                8
            }
        };
        (trailer, length)
    }
}

/// Return the header length once all the optional fields are available.
pub(crate) fn gzip_header(bytes: &[u8]) -> Result<Option<usize>, ()> {
    if bytes.len() >= 2 && !bytes.starts_with(&[0x1f, 0x8b]) {
        return Err(());
    }
    if bytes.len() < 10 {
        return Ok(None);
    }
    let flags = bytes[3];
    if bytes[2] != 8 || flags & 0xe0 != 0 {
        return Err(());
    }
    let mut length = 10;
    if flags & 4 != 0 {
        let Some(extra) = bytes.get(10..12) else {
            return Ok(None);
        };
        length = 12 + usize::from(u16::from_le_bytes([extra[0], extra[1]]));
    }
    for flag in [8, 16] {
        if flags & flag != 0 {
            let Some(end) = bytes
                .get(length..)
                .and_then(|bytes| bytes.iter().position(|byte| *byte == 0))
            else {
                return Ok(None);
            };
            length += end + 1;
        }
    }
    if flags & 2 != 0 {
        let Some(checksum) = bytes.get(length..length + 2) else {
            return Ok(None);
        };
        let mut crc = Crc::new();
        crc.update(&bytes[..length]);
        if crc.sum() as u16 != u16::from_le_bytes([checksum[0], checksum[1]]) {
            return Err(());
        }
        length += 2;
    }
    Ok((length <= bytes.len()).then_some(length))
}

/// Level zero stores unchanged bytes in byte-aligned blocks of at most 65535 bytes.
pub(crate) fn stored(output: &mut Vec<u8>, input: &[u8], final_block: bool) {
    let mut blocks = input.chunks(usize::from(u16::MAX)).peekable();
    if input.is_empty() {
        output.extend([u8::from(final_block), 0, 0, 0xff, 0xff]);
    }
    while let Some(block) = blocks.next() {
        let length = block.len() as u16;
        output.push(u8::from(final_block && blocks.peek().is_none()));
        output.extend(length.to_le_bytes());
        output.extend((!length).to_le_bytes());
        output.extend(block);
    }
}
