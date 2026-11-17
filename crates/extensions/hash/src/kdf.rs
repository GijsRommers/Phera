//! Deriving keys from a password or from key material, with HMAC:
//!
//! ```php
//! hash_pbkdf2('sha256', 'password', 'salt', 1000, 20);  // 20 hexadecimal digits
//! hash_hkdf('sha256', 'input key material', 32);        // 32 raw bytes
//! ```

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::algorithms::Algorithm;
use crate::digests::hex;
use crate::hmac::Hmac;
use crate::{off_thread, options};

pub const FUNCTIONS: &[FunctionEntry] = &[HASH_PBKDF2, HASH_HKDF];

/// HKDF can expand to at most this many digests (RFC 5869, 2.3).
const HKDF_MAX_BLOCKS: usize = 255;

/// How long `algorithm`'s digests are: the length of any one of them.
fn digest_size(algorithm: Algorithm) -> usize {
    algorithm.digest(&[]).len()
}

/// `hash_pbkdf2(string $algo, string $password, string $salt, int $iterations,
/// int $length = 0, bool $binary = false, array $options = []): string`
///
/// `$length` counts hexadecimal digits unless `$binary`, and 0 means one
/// digest. `$options` only matters to the seeded algorithms, none of which
/// is cryptographic, so it is never read.
const HASH_PBKDF2: FunctionEntry = FunctionEntry::asynchronous(
    "hash_pbkdf2",
    &[
        Param::new("algo", ParamType::String),
        Param::new("password", ParamType::String),
        Param::new("salt", ParamType::String),
        Param::new("iterations", ParamType::Int),
        Param::new("length", ParamType::Int).optional(),
        Param::new("binary", ParamType::Bool).optional(),
        Param::new("options", ParamType::Array).optional(),
    ],
    hash_pbkdf2,
);

/// Slow on purpose, like `password_hash()`, so the iterations run off the
/// worker thread.
fn hash_pbkdf2(_: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let algorithm =
            options::cryptographic_algorithm(args::bytes(&arguments, 0), "hash_pbkdf2")?;
        let iterations = args::int(&arguments, 3).unwrap_or_default();
        let length = args::int(&arguments, 4).unwrap_or_default();
        if iterations <= 0 {
            return Err(NativeError::value_error(
                "hash_pbkdf2(): Argument #4 ($iterations) must be greater than 0",
            ));
        }
        if length < 0 {
            return Err(NativeError::value_error(
                "hash_pbkdf2(): Argument #5 ($length) must be greater than or equal to 0",
            ));
        }
        let binary = args::bool(&arguments, 5).unwrap_or(false);
        let size = digest_size(algorithm);
        let length = match (length, binary) {
            (0, true) => size,
            (0, false) => size * 2,
            (length, _) => length as usize,
        };
        let bytes = if binary { length } else { length.div_ceil(2) };
        let password = args::bytes(&arguments, 1).to_vec();
        let salt = args::bytes(&arguments, 2).to_vec();
        let iterations = iterations as u64;
        let derived = off_thread(move || pbkdf2(algorithm, &password, &salt, iterations, bytes));
        let output = derived
            .await?
            .ok_or_else(|| NativeError::error("Insufficient memory"))?;
        if binary {
            Ok(Value::from(output))
        } else {
            // An odd length cuts the last byte's second digit.
            let mut text = hex(&output);
            text.truncate(length);
            Ok(Value::from(text))
        }
    })
}

/// The first `bytes` bytes of PBKDF2 (RFC 8018), or `None` without the
/// memory for them. Block i is the XOR of U1..Un, where U1 is the HMAC of
/// the salt and i, and every next U the HMAC of the one before.
fn pbkdf2(
    algorithm: Algorithm,
    password: &[u8],
    salt: &[u8],
    iterations: u64,
    bytes: usize,
) -> Option<Vec<u8>> {
    let mut output = Vec::new();
    output.try_reserve(bytes).ok()?;
    // The password is the key throughout, so its pads are made once.
    let hmac = Hmac::new(algorithm, password);
    for index in 1..=bytes.div_ceil(digest_size(algorithm)) {
        let mut message = salt.to_vec();
        message.extend_from_slice(&(index as u32).to_be_bytes());
        let mut u = hmac.sign(&message);
        let mut block = u.clone();
        for _ in 1..iterations {
            u = hmac.sign(&u);
            for (byte, next) in block.iter_mut().zip(&u) {
                *byte ^= next;
            }
        }
        output.extend_from_slice(&block);
    }
    output.truncate(bytes);
    Some(output)
}

/// `hash_hkdf(string $algo, string $key, int $length = 0, string $info = "",
/// string $salt = ""): string`, raw bytes; 0 means one digest.
const HASH_HKDF: FunctionEntry = FunctionEntry::sync(
    "hash_hkdf",
    &[
        Param::new("algo", ParamType::String),
        Param::new("key", ParamType::String),
        Param::new("length", ParamType::Int).optional(),
        Param::new("info", ParamType::String).optional(),
        Param::new("salt", ParamType::String).optional(),
    ],
    hash_hkdf,
);

fn hash_hkdf(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let algorithm = options::cryptographic_algorithm(args::bytes(arguments, 0), "hash_hkdf")?;
    let key = args::bytes(arguments, 1);
    if key.is_empty() {
        return Err(NativeError::value_error(
            "hash_hkdf(): Argument #2 ($key) must not be empty",
        ));
    }
    let length = args::int(arguments, 2).unwrap_or_default();
    if length < 0 {
        return Err(NativeError::value_error(
            "hash_hkdf(): Argument #3 ($length) must be greater than or equal to 0",
        ));
    }
    let size = digest_size(algorithm);
    let longest = size * HKDF_MAX_BLOCKS;
    if length as usize > longest {
        return Err(NativeError::value_error(format!(
            "hash_hkdf(): Argument #3 ($length) must be less than or equal to {longest}"
        )));
    }
    let length = if length == 0 { size } else { length as usize };
    // RFC 5869: extract a key with the salt, then expand it with the info.
    let extracted = Hmac::new(algorithm, args::bytes(arguments, 4)).sign(key);
    let expand = Hmac::new(algorithm, &extracted);
    let mut result = Vec::with_capacity(length);
    let mut previous = Vec::new();
    for counter in 1..=length.div_ceil(size) {
        previous.extend_from_slice(args::bytes(arguments, 3));
        previous.push(counter as u8);
        previous = expand.sign(&previous);
        result.extend_from_slice(&previous);
    }
    result.truncate(length);
    Ok(Value::from(result))
}
