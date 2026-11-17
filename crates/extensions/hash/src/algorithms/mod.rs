//! Supported algorithms share one catalogue: `hash_algos()` never advertises a placeholder.
use digest::{Digest, DynDigest};
mod checksums;
mod haval;
mod murmur;
mod snefru;
mod tiger;
pub const NAMES: &[&str] = &[
    "md2",
    "md4",
    "md5",
    "sha1",
    "sha224",
    "sha256",
    "sha384",
    "sha512/224",
    "sha512/256",
    "sha512",
    "sha3-224",
    "sha3-256",
    "sha3-384",
    "sha3-512",
    "ripemd128",
    "ripemd160",
    "ripemd256",
    "ripemd320",
    "whirlpool",
    "tiger128,3",
    "tiger160,3",
    "tiger192,3",
    "tiger128,4",
    "tiger160,4",
    "tiger192,4",
    "snefru",
    "snefru256",
    "gost",
    "gost-crypto",
    "adler32",
    "crc32",
    "crc32b",
    "crc32c",
    "fnv132",
    "fnv1a32",
    "fnv164",
    "fnv1a64",
    "joaat",
    "murmur3a",
    "murmur3c",
    "murmur3f",
    "xxh32",
    "xxh64",
    "xxh3",
    "xxh128",
    "haval128,3",
    "haval160,3",
    "haval192,3",
    "haval224,3",
    "haval256,3",
    "haval128,4",
    "haval160,4",
    "haval192,4",
    "haval224,4",
    "haval256,4",
    "haval128,5",
    "haval160,5",
    "haval192,5",
    "haval224,5",
    "haval256,5",
];

#[derive(Clone)]
pub struct Algorithm {
    pub name: &'static str,
    pub seed: u64,
    pub secret: Option<Vec<u8>>,
}
impl Algorithm {
    pub fn find(name: &[u8]) -> Option<Self> {
        let common = match name {
            b"md5" => Some("md5"),
            b"sha1" => Some("sha1"),
            b"sha256" => Some("sha256"),
            b"xxh3" => Some("xxh3"),
            _ => None,
        };
        if let Some(name) = common {
            return Some(Self {
                name,
                seed: 0,
                secret: None,
            });
        }
        NAMES
            .iter()
            .find(|candidate| candidate.as_bytes().eq_ignore_ascii_case(name))
            .map(|name| Self {
                name,
                seed: 0,
                secret: None,
            })
    }
    pub fn block_size(&self) -> Option<usize> {
        Some(match self.name {
            "md2" => 16,
            "sha384" | "sha512" | "sha512/224" | "sha512/256" => 128,
            "sha3-224" => 144,
            "sha3-256" => 136,
            "sha3-384" => 104,
            "sha3-512" => 72,
            "snefru" | "snefru256" | "gost" | "gost-crypto" => 32,
            n if n.starts_with("haval") => 128,
            n if n.starts_with("md")
                || n.starts_with("sha")
                || n.starts_with("ripemd")
                || n.starts_with("tiger")
                || n == "whirlpool" =>
            {
                64
            }
            _ => return None,
        })
    }
    pub fn digest(&self, data: &[u8]) -> Vec<u8> {
        // Avoid heap-allocating a dynamic context for the common one-shot digests.
        match self.name {
            "md5" => return md5::Md5::digest(data).to_vec(),
            "sha1" => return sha1::Sha1::digest(data).to_vec(),
            "sha256" => return sha2::Sha256::digest(data).to_vec(),
            _ => {}
        }
        if let Some(mut digest) = self.crypto() {
            digest.update(data);
            return digest.finalize().to_vec();
        }
        match self.name {
            "snefru" | "snefru256" => snefru::hash(data),
            n if n.starts_with("haval") || n.starts_with("tiger") => {
                let bits = n[5..8].parse().expect("catalogue digest width");
                let passes = n[9..].parse().expect("catalogue pass count");
                if n.starts_with("haval") {
                    haval::hash(data, bits, passes)
                } else {
                    tiger::hash(data, bits, passes)
                }
            }
            "murmur3a" => murmur::a(data, self.seed as u32),
            "murmur3c" => murmur::c(data, self.seed as u32),
            "murmur3f" => murmur::f(data, self.seed),
            "xxh32" => xxhash_rust::xxh32::xxh32(data, self.seed as u32)
                .to_be_bytes()
                .to_vec(),
            "xxh64" => xxhash_rust::xxh64::xxh64(data, self.seed)
                .to_be_bytes()
                .to_vec(),
            "xxh3" => self
                .secret
                .as_ref()
                .map_or_else(
                    || xxhash_rust::xxh3::xxh3_64_with_seed(data, self.seed),
                    |secret| xxhash_rust::xxh3::xxh3_64_with_secret(data, secret),
                )
                .to_be_bytes()
                .to_vec(),
            "xxh128" => self
                .secret
                .as_ref()
                .map_or_else(
                    || xxhash_rust::xxh3::xxh3_128_with_seed(data, self.seed),
                    |secret| xxhash_rust::xxh3::xxh3_128_with_secret(data, secret),
                )
                .to_be_bytes()
                .to_vec(),
            n => checksums::hash(n, data),
        }
    }
    pub fn crypto(&self) -> Option<Box<dyn DynDigest>> {
        Some(match self.name {
            "md2" => Box::new(md2::Md2::new()),
            "md4" => Box::new(md4::Md4::new()),
            "md5" => Box::new(md5::Md5::new()),
            "sha1" => Box::new(sha1::Sha1::new()),
            "sha224" => Box::new(sha2::Sha224::new()),
            "sha256" => Box::new(sha2::Sha256::new()),
            "sha384" => Box::new(sha2::Sha384::new()),
            "sha512/224" => Box::new(sha2::Sha512_224::new()),
            "sha512/256" => Box::new(sha2::Sha512_256::new()),
            "sha512" => Box::new(sha2::Sha512::new()),
            "sha3-224" => Box::new(sha3::Sha3_224::new()),
            "sha3-256" => Box::new(sha3::Sha3_256::new()),
            "sha3-384" => Box::new(sha3::Sha3_384::new()),
            "sha3-512" => Box::new(sha3::Sha3_512::new()),
            "ripemd128" => Box::new(ripemd::Ripemd128::new()),
            "ripemd160" => Box::new(ripemd::Ripemd160::new()),
            "ripemd256" => Box::new(ripemd::Ripemd256::new()),
            "ripemd320" => Box::new(ripemd::Ripemd320::new()),
            "whirlpool" => Box::new(whirlpool::Whirlpool::new()),
            "gost" => Box::new(gost94::Gost94Test::new()),
            "gost-crypto" => Box::new(gost94::Gost94CryptoPro::new()),
            _ => return None,
        })
    }
}
