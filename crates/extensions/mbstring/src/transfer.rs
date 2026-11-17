//! Transfer formats retain invalid units: `mb_strlen("?", 'BASE64') === 1`.
use crate::decode::BAD;
pub fn base64_decode(bytes: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut bits = 0;
    let mut cache = 0u32;
    for byte in bytes {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\r' | b'\n' | b' ' | b'\t' | b'=' => continue,
            _ => {
                out.push(BAD);
                continue;
            }
        };
        bits += 6;
        cache = (cache << 6) | u32::from(value);
        if bits == 24 {
            out.extend([(cache >> 16) & 255, (cache >> 8) & 255, cache & 255]);
            cache = 0;
            bits = 0;
        }
    }
    match bits {
        18 => out.extend([(cache >> 10) & 255, (cache >> 2) & 255]),
        12 => out.push((cache >> 4) & 255),
        _ => {}
    }
    out
}
pub fn quoted_encode(points: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut column = 0;
    for point in points {
        match point {
            0 => {
                out.push(0);
                column = 0;
                continue;
            }
            10 => {
                out.extend_from_slice(b"\r\n");
                column = 0;
                continue;
            }
            13 => continue,
            _ => {}
        }
        if column >= 72 {
            out.extend_from_slice(b"=\r\n");
            column = 0;
        }
        if *point >= 128 || *point == 61 {
            out.extend_from_slice(format!("={:02X}", point & 255).as_bytes());
            column += 3;
        } else {
            out.push(*point as u8);
            column += 1;
        }
    }
    out
}
