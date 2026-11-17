//! PHP's sixty hash algorithms, under the names `hash()` takes and in the
//! order `hash_algos()` lists them. Names ignore case:
//!
//! ```php
//! hash('crc32b', 'abc');  // "352441c2"
//! hash('CRC32B', 'abc');  // the same
//! hash_algos()[0];        // "md2"
//! ```
//!
//! The common cryptographic hashes come from the RustCrypto crates and
//! xxHash from `xxhash-rust`; the rest are ported from php-src.

use std::sync::LazyLock;

use digest::{Digest, DynDigest};

use crate::options::Options;

mod checksums;
mod haval;
mod murmur;
mod snefru;
mod tiger;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Md2,
    Md4,
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512_224,
    Sha512_256,
    Sha512,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Ripemd128,
    Ripemd160,
    Ripemd256,
    Ripemd320,
    Whirlpool,
    /// Bits of output and passes: `tiger160,4` is `Tiger(160, 4)`.
    Tiger(usize, usize),
    /// PHP calls it both `snefru` and `snefru256`.
    Snefru,
    /// GOST R 34.11-94 with the test parameters.
    Gost,
    /// GOST R 34.11-94 with the CryptoPro parameters.
    GostCrypto,
    Adler32,
    /// The bzip2 CRC: most significant bit first, bytes written backwards.
    Crc32,
    /// The CRC of zlib and PHP's `crc32()`.
    Crc32b,
    /// Castagnoli's polynomial, as in iSCSI.
    Crc32c,
    Fnv132,
    Fnv1a32,
    Fnv164,
    Fnv1a64,
    Joaat,
    Murmur3a,
    Murmur3c,
    Murmur3f,
    Xxh32,
    Xxh64,
    Xxh3,
    Xxh128,
    /// Bits of output and passes: `haval160,4` is `Haval(160, 4)`.
    Haval(usize, usize),
}

use Algorithm::*;

/// Every algorithm under its PHP name, in `hash_algos()` order.
pub const ALGORITHMS: [(&str, Algorithm); 60] = [
    ("md2", Md2),
    ("md4", Md4),
    ("md5", Md5),
    ("sha1", Sha1),
    ("sha224", Sha224),
    ("sha256", Sha256),
    ("sha384", Sha384),
    ("sha512/224", Sha512_224),
    ("sha512/256", Sha512_256),
    ("sha512", Sha512),
    ("sha3-224", Sha3_224),
    ("sha3-256", Sha3_256),
    ("sha3-384", Sha3_384),
    ("sha3-512", Sha3_512),
    ("ripemd128", Ripemd128),
    ("ripemd160", Ripemd160),
    ("ripemd256", Ripemd256),
    ("ripemd320", Ripemd320),
    ("whirlpool", Whirlpool),
    ("tiger128,3", Tiger(128, 3)),
    ("tiger160,3", Tiger(160, 3)),
    ("tiger192,3", Tiger(192, 3)),
    ("tiger128,4", Tiger(128, 4)),
    ("tiger160,4", Tiger(160, 4)),
    ("tiger192,4", Tiger(192, 4)),
    ("snefru", Snefru),
    ("snefru256", Snefru),
    ("gost", Gost),
    ("gost-crypto", GostCrypto),
    ("adler32", Adler32),
    ("crc32", Crc32),
    ("crc32b", Crc32b),
    ("crc32c", Crc32c),
    ("fnv132", Fnv132),
    ("fnv1a32", Fnv1a32),
    ("fnv164", Fnv164),
    ("fnv1a64", Fnv1a64),
    ("joaat", Joaat),
    ("murmur3a", Murmur3a),
    ("murmur3c", Murmur3c),
    ("murmur3f", Murmur3f),
    ("xxh32", Xxh32),
    ("xxh64", Xxh64),
    ("xxh3", Xxh3),
    ("xxh128", Xxh128),
    ("haval128,3", Haval(128, 3)),
    ("haval160,3", Haval(160, 3)),
    ("haval192,3", Haval(192, 3)),
    ("haval224,3", Haval(224, 3)),
    ("haval256,3", Haval(256, 3)),
    ("haval128,4", Haval(128, 4)),
    ("haval160,4", Haval(160, 4)),
    ("haval192,4", Haval(192, 4)),
    ("haval224,4", Haval(224, 4)),
    ("haval256,4", Haval(256, 4)),
    ("haval128,5", Haval(128, 5)),
    ("haval160,5", Haval(160, 5)),
    ("haval192,5", Haval(192, 5)),
    ("haval224,5", Haval(224, 5)),
    ("haval256,5", Haval(256, 5)),
];

/// A name as one number, for a fast lookup: the name's bytes in lowercase,
/// then its length in the lowest byte, so that no two names share a key.
/// `None` for a name too long to be an algorithm's.
fn key(name: &[u8]) -> Option<u128> {
    if name.len() > 15 {
        return None;
    }
    let bytes = name.iter().fold(0u128, |key, byte| {
        key << 8 | u128::from(byte.to_ascii_lowercase())
    });
    Some(bytes << 8 | name.len() as u128)
}

impl Algorithm {
    /// The algorithm PHP knows by `name`, ignoring case.
    pub fn find(name: &[u8]) -> Option<Algorithm> {
        // Sorted by key once, so that every call is a binary search over
        // integers.
        static BY_KEY: LazyLock<Vec<(u128, Algorithm)>> = LazyLock::new(|| {
            let mut sorted: Vec<_> = ALGORITHMS
                .iter()
                .map(|&(name, algorithm)| {
                    (key(name.as_bytes()).expect("names are short"), algorithm)
                })
                .collect();
            sorted.sort_unstable_by_key(|&(key, _)| key);
            sorted
        });
        let key = key(name)?;
        let index = BY_KEY.binary_search_by_key(&key, |&(key, _)| key).ok()?;
        Some(BY_KEY[index].1)
    }

    /// The block size HMAC pads its key to. Only cryptographic hashes have
    /// one, and only they are allowed for HMAC and the key derivations.
    pub fn block_size(self) -> Option<usize> {
        match self {
            Md2 => Some(16),
            Md4 | Md5 | Sha1 | Sha224 | Sha256 => Some(64),
            Sha384 | Sha512_224 | Sha512_256 | Sha512 => Some(128),
            Sha3_224 => Some(144),
            Sha3_256 => Some(136),
            Sha3_384 => Some(104),
            Sha3_512 => Some(72),
            Ripemd128 | Ripemd160 | Ripemd256 | Ripemd320 | Whirlpool | Tiger(..) => Some(64),
            Snefru | Gost | GostCrypto => Some(32),
            Haval(..) => Some(128),
            Adler32 | Crc32 | Crc32b | Crc32c | Fnv132 | Fnv1a32 | Fnv164 | Fnv1a64 | Joaat
            | Murmur3a | Murmur3c | Murmur3f | Xxh32 | Xxh64 | Xxh3 | Xxh128 => None,
        }
    }

    /// Whether `$options['seed']` means anything to this algorithm.
    pub fn takes_seed(self) -> bool {
        matches!(
            self,
            Murmur3a | Murmur3c | Murmur3f | Xxh32 | Xxh64 | Xxh3 | Xxh128
        )
    }

    /// The name for xxh3 and xxh128, the two that take a secret, in the
    /// messages about it; `None` for every other algorithm.
    pub fn secret_name(self) -> Option<&'static str> {
        match self {
            Xxh3 => Some("xxh3"),
            Xxh128 => Some("xxh128"),
            _ => None,
        }
    }

    /// A hasher that takes its input in pieces, for the algorithms that
    /// come from RustCrypto. The others are only computed in one go, so
    /// `hash_update()` keeps their input until `hash_final()`.
    pub fn streaming(self) -> Option<Box<dyn DynDigest>> {
        Some(match self {
            Md2 => Box::new(md2::Md2::new()),
            Md4 => Box::new(md4::Md4::new()),
            Md5 => Box::new(md5::Md5::new()),
            Sha1 => Box::new(sha1::Sha1::new()),
            Sha224 => Box::new(sha2::Sha224::new()),
            Sha256 => Box::new(sha2::Sha256::new()),
            Sha384 => Box::new(sha2::Sha384::new()),
            Sha512_224 => Box::new(sha2::Sha512_224::new()),
            Sha512_256 => Box::new(sha2::Sha512_256::new()),
            Sha512 => Box::new(sha2::Sha512::new()),
            Sha3_224 => Box::new(sha3::Sha3_224::new()),
            Sha3_256 => Box::new(sha3::Sha3_256::new()),
            Sha3_384 => Box::new(sha3::Sha3_384::new()),
            Sha3_512 => Box::new(sha3::Sha3_512::new()),
            Ripemd128 => Box::new(ripemd::Ripemd128::new()),
            Ripemd160 => Box::new(ripemd::Ripemd160::new()),
            Ripemd256 => Box::new(ripemd::Ripemd256::new()),
            Ripemd320 => Box::new(ripemd::Ripemd320::new()),
            Whirlpool => Box::new(whirlpool::Whirlpool::new()),
            Gost => Box::new(gost94::Gost94Test::new()),
            GostCrypto => Box::new(gost94::Gost94CryptoPro::new()),
            _ => return None,
        })
    }

    /// The digest of `data`, for an algorithm that takes no options.
    pub fn digest(self, data: &[u8]) -> Vec<u8> {
        self.digest_with(&Options::default(), data)
    }

    /// The digest of `data` with a seed or secret from `hash()`'s options.
    pub fn digest_with(self, options: &Options, data: &[u8]) -> Vec<u8> {
        use xxhash_rust::{xxh3, xxh32, xxh64};

        /// A RustCrypto digest in one go, without the box of `streaming()`.
        fn once<D: Digest>(data: &[u8]) -> Vec<u8> {
            D::digest(data).to_vec()
        }

        // The 32-bit hashes use the low half of PHP's 64-bit seed.
        let seed = options.seed;
        let secret = options.secret.as_deref();
        match self {
            Md2 => once::<md2::Md2>(data),
            Md4 => once::<md4::Md4>(data),
            Md5 => once::<md5::Md5>(data),
            Sha1 => once::<sha1::Sha1>(data),
            Sha224 => once::<sha2::Sha224>(data),
            Sha256 => once::<sha2::Sha256>(data),
            Sha384 => once::<sha2::Sha384>(data),
            Sha512_224 => once::<sha2::Sha512_224>(data),
            Sha512_256 => once::<sha2::Sha512_256>(data),
            Sha512 => once::<sha2::Sha512>(data),
            Sha3_224 => once::<sha3::Sha3_224>(data),
            Sha3_256 => once::<sha3::Sha3_256>(data),
            Sha3_384 => once::<sha3::Sha3_384>(data),
            Sha3_512 => once::<sha3::Sha3_512>(data),
            Ripemd128 => once::<ripemd::Ripemd128>(data),
            Ripemd160 => once::<ripemd::Ripemd160>(data),
            Ripemd256 => once::<ripemd::Ripemd256>(data),
            Ripemd320 => once::<ripemd::Ripemd320>(data),
            Whirlpool => once::<whirlpool::Whirlpool>(data),
            Gost => once::<gost94::Gost94Test>(data),
            GostCrypto => once::<gost94::Gost94CryptoPro>(data),
            Tiger(bits, passes) => tiger::hash(data, bits, passes),
            Snefru => snefru::hash(data),
            Haval(bits, passes) => haval::hash(data, bits, passes),
            Adler32 => checksums::adler32(data).to_vec(),
            Crc32 => checksums::crc32_bzip2(data).to_vec(),
            Crc32b => checksums::crc32b(data).to_vec(),
            Crc32c => checksums::crc32c(data).to_vec(),
            Fnv132 => checksums::fnv1_32(data).to_vec(),
            Fnv1a32 => checksums::fnv1a_32(data).to_vec(),
            Fnv164 => checksums::fnv1_64(data).to_vec(),
            Fnv1a64 => checksums::fnv1a_64(data).to_vec(),
            Joaat => checksums::joaat(data).to_vec(),
            Murmur3a => murmur::a(data, seed as u32),
            Murmur3c => murmur::c(data, seed as u32),
            Murmur3f => murmur::f(data, seed),
            Xxh32 => xxh32::xxh32(data, seed as u32).to_be_bytes().to_vec(),
            Xxh64 => xxh64::xxh64(data, seed).to_be_bytes().to_vec(),
            Xxh3 => match secret {
                Some(secret) => xxh3::xxh3_64_with_secret(data, secret),
                None => xxh3::xxh3_64_with_seed(data, seed),
            }
            .to_be_bytes()
            .to_vec(),
            Xxh128 => match secret {
                Some(secret) => xxh3::xxh3_128_with_secret(data, secret),
                None => xxh3::xxh3_128_with_seed(data, seed),
            }
            .to_be_bytes()
            .to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_is_found_in_any_case() {
        for (name, algorithm) in ALGORITHMS {
            assert_eq!(Algorithm::find(name.as_bytes()), Some(algorithm));
            let uppercase = name.to_ascii_uppercase();
            assert_eq!(Algorithm::find(uppercase.as_bytes()), Some(algorithm));
        }
        for unknown in [
            "",
            "md",
            "md55",
            "md5\0",
            "sha512/",
            "gost-crypto2",
            "a-name-longer-than-any",
        ] {
            assert_eq!(Algorithm::find(unknown.as_bytes()), None, "{unknown}");
        }
    }
}
