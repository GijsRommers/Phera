//! PHP uses GB18030-2000: `bin2hex(mb_chr(0xE78D, "GB18030")) === "a6d9"`.
use crate::encoding::read_u32;
const PATCHES: &[u8] = include_bytes!("../data/gb18030-patches.bin");
pub fn encode(cp: u32, out: &mut Vec<u8>) -> bool {
    if let Some(record) = PATCHES
        .as_chunks::<9>()
        .0
        .iter()
        .find(|r| read_u32(&r[..]) == cp)
    {
        out.extend_from_slice(&record[5..5 + usize::from(record[4])]);
        true
    } else {
        false
    }
}
pub fn decode(bytes: &[u8]) -> Option<u32> {
    PATCHES
        .as_chunks::<9>()
        .0
        .iter()
        .find(|r| &r[5..5 + usize::from(r[4])] == bytes)
        .map(|r| read_u32(&r[..]))
}
