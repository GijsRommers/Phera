//! PHP's SJIS and CP932 mappings differ: tables retain the oracle's exact mappings.
use crate::{
    decode::BAD,
    encoding::{Encoding, read_u32},
};
fn tables(encoding: Encoding) -> Option<(&'static [u8], &'static [u8])> {
    match encoding.0 {
        16 => Some((
            include_bytes!("../data/decode-EUC-JP.bin"),
            include_bytes!("../data/encode-EUC-JP.bin"),
        )),
        17 => Some((
            include_bytes!("../data/decode-SJIS.bin"),
            include_bytes!("../data/encode-SJIS.bin"),
        )),
        18 => Some((
            include_bytes!("../data/decode-CP932.bin"),
            include_bytes!("../data/encode-CP932.bin"),
        )),
        19 => Some((
            include_bytes!("../data/decode-SJIS-win.bin"),
            include_bytes!("../data/encode-SJIS-win.bin"),
        )),
        38 => Some((
            include_bytes!("../data/decode-CP936.bin"),
            include_bytes!("../data/encode-CP936.bin"),
        )),
        39 => Some((
            include_bytes!("../data/decode-BIG-5.bin"),
            include_bytes!("../data/encode-BIG-5.bin"),
        )),
        40 => Some((
            include_bytes!("../data/decode-EUC-KR.bin"),
            include_bytes!("../data/encode-EUC-KR.bin"),
        )),
        _ => None,
    }
}
pub fn decode(bytes: &[u8], encoding: Encoding) -> Option<Vec<u32>> {
    let (table, _) = tables(encoding)?;
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if encoding.0 == 16 && bytes[i] == 0x8f && i + 2 < bytes.len() {
            // JIS X 0212 uses a three-byte EUC-JP sequence.
            let web = encoding_rs::EUC_JP
                .decode_without_bom_handling_and_without_replacement(&bytes[i..i + 3]);
            out.push(web.and_then(|s| s.chars().next()).map_or(BAD, u32::from));
            i += 3;
            continue;
        }
        let second = bytes.get(i + 1).copied().unwrap_or(0);
        let offset = (usize::from(bytes[i]) * 256 + usize::from(second)) * 4;
        let entry = read_u32(&table[offset..]);
        let cp = entry & 0x7fffffff;
        out.push(if cp == 0x7fffffff { BAD } else { cp });
        i += if entry & 0x80000000 != 0 { 2 } else { 1 };
    }
    Some(out)
}
pub fn encode(cp: u32, encoding: Encoding, out: &mut Vec<u8>) -> Option<bool> {
    let (_, table) = tables(encoding)?;
    let (mut low, mut high) = (0, table.len() / 9);
    while low < high {
        let mid = (low + high) / 2;
        let record = &table[mid * 9..][..9];
        match read_u32(record).cmp(&cp) {
            std::cmp::Ordering::Equal => {
                out.extend_from_slice(&record[5..5 + usize::from(record[4])]);
                return Some(true);
            }
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
        }
    }
    Some(false)
}
