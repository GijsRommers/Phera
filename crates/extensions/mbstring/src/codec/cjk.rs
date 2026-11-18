//! The double-byte East Asian encodings, from tables generated with PHP, so
//! each keeps PHP's own variant mappings:
//!
//! ```php
//! bin2hex(mb_convert_encoding('①', 'SJIS', 'UTF-8'));   // "3f": not in plain Shift_JIS
//! bin2hex(mb_convert_encoding('①', 'CP932', 'UTF-8'));  // "8740": Microsoft's extension
//! ```
//!
//! A decode table has an entry for every pair of bytes: the code point of the
//! character they start, and whether it takes one byte or both. An encode
//! table is a sorted list of code points with the bytes for each.

use super::BAD;
use crate::encoding::read_u32;

pub struct Tables {
    /// 65,536 little-endian `u32`s, one for each pair of bytes: the code
    /// point, plus [`TWO_BYTES`] when the character takes both.
    decode: &'static [u8],
    /// Records of nine bytes, sorted by code point: the code point, the
    /// number of bytes, and up to four bytes.
    encode: &'static [u8],
    /// EUC-JP's 0x8F starts a three-byte character of JIS X 0212, which
    /// the pair table cannot hold.
    three_byte_lead: Option<u8>,
}

/// The flag in a decode entry for a character of two bytes.
const TWO_BYTES: u32 = 0x8000_0000;
/// The decode entry's code point when the bytes start no character.
const NO_CHARACTER: u32 = 0x7FFF_FFFF;

pub static EUC_JP: Tables = Tables {
    decode: include_bytes!("../../data/decode-EUC-JP.bin"),
    encode: include_bytes!("../../data/encode-EUC-JP.bin"),
    three_byte_lead: Some(0x8F),
};
pub static SJIS: Tables = Tables {
    decode: include_bytes!("../../data/decode-SJIS.bin"),
    encode: include_bytes!("../../data/encode-SJIS.bin"),
    three_byte_lead: None,
};
pub static CP932: Tables = Tables {
    decode: include_bytes!("../../data/decode-CP932.bin"),
    encode: include_bytes!("../../data/encode-CP932.bin"),
    three_byte_lead: None,
};
pub static SJIS_WIN: Tables = Tables {
    decode: include_bytes!("../../data/decode-SJIS-win.bin"),
    encode: include_bytes!("../../data/encode-SJIS-win.bin"),
    three_byte_lead: None,
};
pub static CP936: Tables = Tables {
    decode: include_bytes!("../../data/decode-CP936.bin"),
    encode: include_bytes!("../../data/encode-CP936.bin"),
    three_byte_lead: None,
};
pub static BIG5: Tables = Tables {
    decode: include_bytes!("../../data/decode-BIG-5.bin"),
    encode: include_bytes!("../../data/encode-BIG-5.bin"),
    three_byte_lead: None,
};
pub static EUC_KR: Tables = Tables {
    decode: include_bytes!("../../data/decode-EUC-KR.bin"),
    encode: include_bytes!("../../data/encode-EUC-KR.bin"),
    three_byte_lead: None,
};

pub fn decode(bytes: &[u8], tables: &Tables) -> Vec<u32> {
    let mut points = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let lead = bytes[index];
        if Some(lead) == tables.three_byte_lead && index + 3 <= bytes.len() {
            let character = encoding_rs::EUC_JP
                .decode_without_bom_handling_and_without_replacement(&bytes[index..index + 3])
                .and_then(|text| text.chars().next());
            points.push(character.map_or(BAD, u32::from));
            index += 3;
            continue;
        }
        let second = bytes.get(index + 1).copied().unwrap_or(0);
        let entry = read_u32(&tables.decode[(usize::from(lead) * 256 + usize::from(second)) * 4..]);
        let point = entry & !TWO_BYTES;
        points.push(if point == NO_CHARACTER { BAD } else { point });
        index += if entry & TWO_BYTES != 0 { 2 } else { 1 };
    }
    points
}

pub fn encode(point: u32, tables: &Tables, output: &mut Vec<u8>) -> bool {
    let records = tables.encode.as_chunks::<9>().0;
    let Ok(found) = records.binary_search_by_key(&point, |record| read_u32(record)) else {
        return false;
    };
    let record = &records[found];
    output.extend_from_slice(&record[5..5 + usize::from(record[4])]);
    true
}
