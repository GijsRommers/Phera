//! Substitution affects invalid and unrepresentable input: `mb_substitute_character('none')`.
use crate::decode::BAD;
use crate::encoding::{Encoding, read_u32};

#[derive(Clone, Debug)]
pub enum Substitute {
    Character(u32),
    None,
    Long(u32),
    Entity(u32),
}
impl Default for Substitute {
    fn default() -> Self {
        Self::Character(63)
    }
}

pub fn encode(points: &[u32], encoding: Encoding, substitute: &Substitute) -> Vec<u8> {
    if encoding.0 == 20 {
        return crate::iso2022::encode(points, substitute);
    }
    if encoding.0 >= 44 {
        return crate::pseudo::encode(points, encoding);
    }
    let mut output = Vec::with_capacity(points.len());
    for &point in points {
        if !one(point, encoding, &mut output) {
            if point == BAD
                && let Substitute::Long(cp) | Substitute::Entity(cp) = substitute
            {
                one(*cp, encoding, &mut output);
                continue;
            }

            let replacement = match substitute {
                Substitute::None => String::new(),
                Substitute::Long(_) if point != BAD => format!("U+{point:04X}"),
                Substitute::Entity(_) if point != BAD => format!("&#x{point:X};"),
                Substitute::Character(cp) => char::from_u32(*cp).unwrap_or('?').to_string(),
                _ => "?".to_string(),
            };
            for c in replacement.chars() {
                if !one(u32::from(c), encoding, &mut output) {
                    one(63, encoding, &mut output);
                }
            }
        }
    }
    output
}
fn one(point: u32, encoding: Encoding, out: &mut Vec<u8>) -> bool {
    if encoding.0 == 21 && crate::gb18030::encode(point, out) {
        return true;
    }
    if let Some(result) = crate::cjk::encode(point, encoding, out) {
        return result;
    }
    let Some(c) = char::from_u32(point) else {
        return false;
    };
    if let Some(table) = encoding.single() {
        if let Some(index) = table
            .as_chunks::<4>()
            .0
            .iter()
            .position(|chunk| read_u32(chunk) == point)
        {
            out.push(index as u8);
            return true;
        }
        return false;
    }
    match encoding.0 {
        0 | 15 if point < 128 => out.push(point as u8),
        1 if point < 256 => out.push(point as u8),
        2..=4 | 8..=10 => {
            out.extend_from_slice(&if matches!(encoding.0, 4 | 10) {
                point.to_le_bytes()
            } else {
                point.to_be_bytes()
            });
        }
        5..=7 if point <= 0xffff => {
            out.extend_from_slice(&if encoding.0 == 7 {
                (point as u16).to_le_bytes()
            } else {
                (point as u16).to_be_bytes()
            });
        }
        11..=13 => {
            for unit in c.encode_utf16(&mut [0; 2]) {
                out.extend_from_slice(&if encoding.0 == 13 {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
        }
        14 => {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
        _ => {
            let Some(web) = encoding.web() else {
                return false;
            };
            let mut buf = [0; 4];
            let (bytes, _, errors) = web.encode(c.encode_utf8(&mut buf));
            if errors {
                return false;
            }
            out.extend_from_slice(&bytes);
        }
    }
    true
}
