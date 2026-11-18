//! Checksums and simple non-cryptographic hashes. PHP has three CRC32s,
//! and only `crc32b` is the one its `crc32()` function computes:
//!
//! ```php
//! hash('crc32', 'abc');   // "73bb8c64"
//! hash('crc32b', 'abc');  // "352441c2", dechex(crc32('abc'))
//! ```
//!
//! Ported from php-src/ext/hash/hash_{crc32,adler32,fnv,joaat}.c; see LICENSE-PHP.

/// Adler-32's modulus, the largest prime below 2^16.
const ADLER_MODULUS: u32 = 65521;

pub fn adler32(data: &[u8]) -> [u8; 4] {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % ADLER_MODULUS;
        b = (b + a) % ADLER_MODULUS;
    }
    ((b << 16) | a).to_be_bytes()
}

/// Bob Jenkins's one-at-a-time hash.
pub fn joaat(data: &[u8]) -> [u8; 4] {
    let mut h = 0u32;
    for &byte in data {
        h = h.wrapping_add(u32::from(byte));
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15).to_be_bytes()
}

const FNV32_OFFSET: u32 = 0x811c9dc5;
const FNV32_PRIME: u32 = 0x01000193;
const FNV64_OFFSET: u64 = 0xcbf29ce484222325;
const FNV64_PRIME: u64 = 0x100000001b3;

/// FNV-1: multiply, then mix in the byte.
pub fn fnv1_32(data: &[u8]) -> [u8; 4] {
    let h = data.iter().fold(FNV32_OFFSET, |h, &byte| {
        h.wrapping_mul(FNV32_PRIME) ^ u32::from(byte)
    });
    h.to_be_bytes()
}

/// FNV-1a: mix in the byte, then multiply.
pub fn fnv1a_32(data: &[u8]) -> [u8; 4] {
    let h = data.iter().fold(FNV32_OFFSET, |h, &byte| {
        (h ^ u32::from(byte)).wrapping_mul(FNV32_PRIME)
    });
    h.to_be_bytes()
}

pub fn fnv1_64(data: &[u8]) -> [u8; 8] {
    let h = data.iter().fold(FNV64_OFFSET, |h, &byte| {
        h.wrapping_mul(FNV64_PRIME) ^ u64::from(byte)
    });
    h.to_be_bytes()
}

pub fn fnv1a_64(data: &[u8]) -> [u8; 8] {
    let h = data.iter().fold(FNV64_OFFSET, |h, &byte| {
        (h ^ u64::from(byte)).wrapping_mul(FNV64_PRIME)
    });
    h.to_be_bytes()
}

/// PHP's `crc32`: the bzip2 CRC, most significant bit first. PHP writes the
/// result in little-endian order, unlike every other algorithm here.
pub fn crc32_bzip2(data: &[u8]) -> [u8; 4] {
    const POLYNOMIAL: u32 = 0x04c11db7;
    let mut h = u32::MAX;
    for &byte in data {
        h ^= u32::from(byte) << 24;
        for _ in 0..8 {
            // Subtract the polynomial when the top bit is set, without a branch.
            h = (h << 1) ^ (POLYNOMIAL & 0u32.wrapping_sub(h >> 31));
        }
    }
    (!h).to_le_bytes()
}

/// PHP's `crc32b`, the CRC of zlib, PNG and Ethernet.
pub fn crc32b(data: &[u8]) -> [u8; 4] {
    reflected_crc32(data, 0xedb88320)
}

/// PHP's `crc32c`, with Castagnoli's polynomial.
pub fn crc32c(data: &[u8]) -> [u8; 4] {
    reflected_crc32(data, 0x82f63b78)
}

/// A CRC that takes the least significant bit first; `polynomial` is
/// written in that order too.
fn reflected_crc32(data: &[u8], polynomial: u32) -> [u8; 4] {
    let mut h = u32::MAX;
    for &byte in data {
        h ^= u32::from(byte);
        for _ in 0..8 {
            h = (h >> 1) ^ (polynomial & 0u32.wrapping_sub(h & 1));
        }
    }
    (!h).to_be_bytes()
}
