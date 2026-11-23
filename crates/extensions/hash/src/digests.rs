//! Hashing a string in one call, the lists of algorithms, and comparing
//! two digests:
//!
//! ```php
//! hash('crc32b', 'abc');                   // "352441c2"
//! hash_hmac('sha256', 'message', 'key');   // 64 hexadecimal digits
//! md5('abc', true);                        // 16 raw bytes
//! hash_equals($stored, $computed);         // in constant time
//! ```

use digest::Digest;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value, args,
};
use subtle::ConstantTimeEq;

use crate::algorithms::ALGORITHMS;
use crate::given;
use crate::hmac::Hmac;
use crate::options::{self, Options};

pub const FUNCTIONS: &[FunctionEntry] = &[
    HASH,
    HASH_HMAC,
    HASH_ALGOS,
    HASH_HMAC_ALGOS,
    HASH_EQUALS,
    MD5,
    SHA1,
];

/// A digest as PHP returns it: the raw bytes, or lowercase hexadecimal.
pub fn output(digest: Vec<u8>, binary: bool) -> Value {
    if binary {
        Value::from(digest)
    } else {
        Value::from(hex(&digest))
    }
}

/// Lowercase hexadecimal, two digits per byte.
pub fn hex(bytes: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = vec![0; bytes.len() * 2];
    for (pair, byte) in text.as_chunks_mut::<2>().0.iter_mut().zip(bytes) {
        *pair = [
            DIGITS[usize::from(byte >> 4)],
            DIGITS[usize::from(byte & 0x0f)],
        ];
    }
    text
}

/// `hash(string $algo, string $data, bool $binary = false, array $options = []): string`
const HASH: FunctionEntry = FunctionEntry::sync(
    "hash",
    &[
        Param::new("algo", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
        Param::new("options", ParamType::Array).optional(),
    ],
    hash,
);

fn hash(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::algorithm(args::bytes(arguments, 0), "hash")?;
    let options = Options::read(context, algorithm, arguments.get(3), "hash")?;
    let digest = algorithm.digest_with(&options, args::bytes(arguments, 1));
    Ok(output(digest, args::bool(arguments, 2).unwrap_or(false)))
}

/// `hash_hmac(string $algo, string $data, string $key, bool $binary = false): string`
const HASH_HMAC: FunctionEntry = FunctionEntry::sync(
    "hash_hmac",
    &[
        Param::new("algo", ParamType::String),
        Param::new("data", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    hash_hmac,
);

fn hash_hmac(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::cryptographic_algorithm(args::bytes(arguments, 0), "hash_hmac")?;
    let hmac = Hmac::new(algorithm, args::bytes(arguments, 2));
    let digest = hmac.sign(args::bytes(arguments, 1));
    Ok(output(digest, args::bool(arguments, 3).unwrap_or(false)))
}

/// `hash_algos(): array`, all sixty names.
const HASH_ALGOS: FunctionEntry = FunctionEntry::sync("hash_algos", &[], hash_algos);

fn hash_algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    list(ALGORITHMS.iter().map(|&(name, _)| name))
}

/// `hash_hmac_algos(): array`, the cryptographic algorithms only.
const HASH_HMAC_ALGOS: FunctionEntry = FunctionEntry::sync("hash_hmac_algos", &[], hash_hmac_algos);

fn hash_hmac_algos(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    let cryptographic = ALGORITHMS
        .iter()
        .filter(|(_, algorithm)| algorithm.block_size().is_some());
    list(cryptographic.map(|&(name, _)| name))
}

fn list(names: impl Iterator<Item = &'static str>) -> NativeResult {
    let mut list = PhpArray::new();
    for name in names {
        list.push(Value::from(name))?;
    }
    Ok(Value::from(list))
}

/// `hash_equals(string $known_string, string $user_string): bool`
///
/// PHP declares both parameters as strings but never converts them: an int
/// is a `TypeError` even without `strict_types`, so they are taken as
/// `mixed` and checked here.
const HASH_EQUALS: FunctionEntry = FunctionEntry::sync(
    "hash_equals",
    &[
        Param::new("known_string", ParamType::Mixed),
        Param::new("user_string", ParamType::Mixed),
    ],
    hash_equals,
);

fn hash_equals(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    for (index, name) in ["known_string", "user_string"].into_iter().enumerate() {
        let value = &arguments[index];
        if !matches!(value, Value::String(_)) {
            return Err(NativeError::type_error(format!(
                "hash_equals(): Argument #{} (${name}) must be of type string, {} given",
                index + 1,
                given(value)
            )));
        }
    }
    // The time taken must not tell an attacker how many bytes matched.
    let equal = args::bytes(arguments, 0).ct_eq(args::bytes(arguments, 1));
    Ok(Value::Bool(equal.into()))
}

/// `md5(string $string, bool $binary = false): string`. It and `sha1()`
/// name their algorithm, so they call it directly instead of through
/// `Algorithm`'s dispatch.
const MD5: FunctionEntry = FunctionEntry::sync(
    "md5",
    &[
        Param::new("string", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    md5,
);

fn md5(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let digest = md5::Md5::digest(args::bytes(arguments, 0)).to_vec();
    Ok(output(digest, args::bool(arguments, 1).unwrap_or(false)))
}

/// `sha1(string $string, bool $binary = false): string`
const SHA1: FunctionEntry = FunctionEntry::sync(
    "sha1",
    &[
        Param::new("string", ParamType::String),
        Param::new("binary", ParamType::Bool).optional(),
    ],
    sha1,
);

fn sha1(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let digest = sha1::Sha1::digest(args::bytes(arguments, 0)).to_vec();
    Ok(output(digest, args::bool(arguments, 1).unwrap_or(false)))
}
