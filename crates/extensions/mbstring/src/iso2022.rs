//! JIS escape modes are preserved: `mb_convert_encoding("日本語", "ISO-2022-JP")`.
use crate::{decode::BAD, encode::Substitute, encoding::Encoding};
#[derive(Clone, Copy)]
enum Mode {
    Ascii,
    Jis,
    Roman,
}
pub fn encode(points: &[u32], substitute: &Substitute) -> Vec<u8> {
    let mut output = Vec::new();
    let mut jis = false;
    let mut roman = false;
    for cp in points {
        if *cp == 0xa5 {
            if !roman {
                output.extend_from_slice(b"\x1b(J");
                roman = true;
                jis = false;
            }
            output.push(b'\\');
            continue;
        }
        if roman {
            output.extend_from_slice(b"\x1b(B");
            roman = false;
        }
        let mut sjis = crate::encode::encode(&[*cp], Encoding(17), substitute);
        if sjis.len() == 1 && sjis[0] >= 128 {
            sjis = crate::encode::encode(&[*cp], Encoding(15), substitute);
        }
        let mut i = 0;
        while i < sjis.len() {
            let b = sjis[i];
            if b < 128 {
                if jis {
                    output.extend_from_slice(b"\x1b(B");
                    jis = false;
                }
                output.push(b);
                i += 1;
            } else if i + 1 < sjis.len() {
                let trail = sjis[i + 1];
                let mut row = (b - if b < 0xa0 { 0x81 } else { 0xc1 }) * 2 + 0x21;
                let column = if trail >= 0x9f {
                    row += 1;
                    trail - 0x7e
                } else {
                    trail - if trail > 0x7e { 0x20 } else { 0x1f }
                };
                if !jis {
                    output.extend_from_slice(b"\x1b$B");
                    jis = true;
                }
                output.extend([row, column]);
                i += 2;
            } else {
                i += 1;
            }
        }
    }
    if jis || roman {
        output.extend_from_slice(b"\x1b(B");
    }
    output
}
pub fn decode(bytes: &[u8]) -> Vec<u32> {
    let mut output = Vec::new();
    let mut mode = Mode::Ascii;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            match bytes.get(i + 1..i + 3) {
                Some(b"$B" | b"$@") => mode = Mode::Jis,
                Some(b"(B" | b"(H") => mode = Mode::Ascii,
                Some(b"(J") => mode = Mode::Roman,
                _ => {
                    i += 1;
                    continue;
                }
            }
            i += 3;
            continue;
        }
        if matches!(bytes[i], 0xe | 0xf) {
            i += 1;
            continue;
        }
        match mode {
            Mode::Ascii | Mode::Roman => {
                output.push(if matches!(mode, Mode::Roman) && bytes[i] == b'\\' {
                    0xa5
                } else if matches!(mode, Mode::Roman) && bytes[i] == b'~' {
                    0x203e
                } else if bytes[i] < 128 {
                    u32::from(bytes[i])
                } else {
                    BAD
                });
                i += 1;
            }
            Mode::Jis => {
                if i + 1 >= bytes.len() {
                    output.push(BAD);
                    break;
                }
                let row = bytes[i];
                let col = bytes[i + 1];
                if !(0x21..=0x7e).contains(&row) || !(0x21..=0x7e).contains(&col) {
                    output.push(BAD);
                    i += 2;
                    continue;
                }
                let lead = (row - 0x21) / 2;
                let lead = lead + if lead < 0x1f { 0x81 } else { 0xc1 };
                let trail = if row & 1 == 0 {
                    col + 0x7e
                } else {
                    col + if col >= 0x60 { 0x20 } else { 0x1f }
                };
                output.extend(crate::cjk::decode(&[lead, trail], Encoding(17)).unwrap_or_default());
                i += 2;
            }
        }
    }
    output
}
