//! Non-cryptographic hashes: `hash('crc32', $s)` differs from `hash('crc32b', $s)`.
//! Ported from php-src/ext/hash/hash_{crc32,adler32,fnv,joaat}.c; see LICENSE-PHP.
pub fn hash(name: &str, data: &[u8]) -> Vec<u8> {
    match name {
        "adler32" => {
            let (mut a, mut b) = (1u32, 0u32);
            for &byte in data {
                a = (a + u32::from(byte)) % 65521;
                b = (b + a) % 65521;
            }
            ((b << 16) | a).to_be_bytes().to_vec()
        }
        "joaat" => {
            let mut h = 0u32;
            for &byte in data {
                h = h.wrapping_add(u32::from(byte));
                h = h.wrapping_add(h << 10);
                h ^= h >> 6;
            }
            h = h.wrapping_add(h << 3);
            h ^= h >> 11;
            h.wrapping_add(h << 15).to_be_bytes().to_vec()
        }
        "fnv132" | "fnv1a32" => {
            let mut h = 0x811c9dc5u32;
            for &byte in data {
                if name == "fnv1a32" {
                    h ^= u32::from(byte);
                }
                h = h.wrapping_mul(0x01000193);
                if name == "fnv132" {
                    h ^= u32::from(byte);
                }
            }
            h.to_be_bytes().to_vec()
        }
        "fnv164" | "fnv1a64" => {
            let mut h = 0xcbf29ce484222325u64;
            for &byte in data {
                if name == "fnv1a64" {
                    h ^= u64::from(byte);
                }
                h = h.wrapping_mul(0x100000001b3);
                if name == "fnv164" {
                    h ^= u64::from(byte);
                }
            }
            h.to_be_bytes().to_vec()
        }
        _ => {
            let mut h = u32::MAX;
            for &byte in data {
                if name == "crc32" {
                    h ^= u32::from(byte) << 24;
                    for _ in 0..8 {
                        h = (h << 1) ^ (0x04c11db7 & 0u32.wrapping_sub(h >> 31));
                    }
                } else {
                    h ^= u32::from(byte);
                    let polynomial = if name == "crc32c" {
                        0x82f63b78
                    } else {
                        0xedb88320
                    };
                    for _ in 0..8 {
                        h = (h >> 1) ^ (polynomial & 0u32.wrapping_sub(h & 1));
                    }
                }
            }
            if name == "crc32" {
                (!h).to_le_bytes().to_vec()
            } else {
                (!h).to_be_bytes().to_vec()
            }
        }
    }
}
