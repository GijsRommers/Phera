//! Byte slicing preserves malformed bytes for fixed-width encodings and split/trim.
use crate::encoding::Encoding;

pub fn offsets(bytes: &[u8], encoding: Encoding) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let size = match encoding.0 {
            2..=4 | 8..=10 => 4,
            5..=7 => 2,
            11..=13 => {
                if i + 1 < bytes.len() {
                    let unit = if encoding.0 == 13 {
                        u16::from_le_bytes([b, bytes[i + 1]])
                    } else {
                        u16::from_be_bytes([b, bytes[i + 1]])
                    };
                    if (0xd800..=0xdbff).contains(&unit) {
                        4
                    } else {
                        2
                    }
                } else {
                    2
                }
            }
            14 => match b {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 1,
            },
            16 => match b {
                0x8f => 3,
                0x8e | 0xa1..=0xfe => 2,
                _ => 1,
            },
            17 => {
                if matches!(b, 0x81..=0x9f | 0xe0..=0xef) {
                    2
                } else {
                    1
                }
            }
            18..=19 => {
                if matches!(b, 0x81..=0x9f | 0xe0..=0xff) {
                    2
                } else {
                    1
                }
            }
            38..=39 if (0x81..=0xfe).contains(&b) => 2,
            40 if (0xa1..=0xfe).contains(&b) => 2,
            21 if (0x81..=0xfe).contains(&b) => {
                if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                    4
                } else {
                    2
                }
            }
            _ => 1,
        };
        i += size;
        offsets.push(i);
    }
    offsets
}
pub fn fixed(encoding: Encoding) -> bool {
    matches!(encoding.0, 0..=10 | 15 | 22..=37 | 41..=43)
}
