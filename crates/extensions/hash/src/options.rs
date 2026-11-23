//! The `$algo` and `$options` arguments of `hash()`, `hash_init()`,
//! `hash_file()` and `hash_pbkdf2()`. Murmur3 and xxHash take a seed, xxh3
//! and xxh128 a secret instead; other algorithms ignore both:
//!
//! ```php
//! hash('xxh3', 'abc', false, ['seed' => 42]);              // "d8438def21bbdcc3"
//! hash('xxh3', 'abc', false, ['secret' => str_repeat('s', 136)]);
//! hash('sha256', 'abc', false, ['seed' => 42]);            // the seed is ignored
//! ```

use phera_extension_api::{ArrayKey, Context, NativeError, Value, value::to_php_string};

use crate::algorithms::Algorithm;

/// xxHash's smallest secret.
const MIN_SECRET: usize = 136;

/// PHP keeps at most this much of a secret; xxHash reads no more.
const MAX_SECRET: usize = 256;

/// The seed or secret an options array sets.
#[derive(Clone, Default)]
pub struct Options {
    /// PHP's int seed as its bits; the 32-bit hashes use the low half.
    pub seed: u64,
    pub secret: Option<Vec<u8>>,
}

/// The algorithm `$algo` names. `function` is the PHP function, for the error.
pub fn algorithm(name: &[u8], function: &str) -> Result<Algorithm, NativeError> {
    Algorithm::find(name).ok_or_else(|| {
        NativeError::value_error(format!(
            "{function}(): Argument #1 ($algo) must be a valid hashing algorithm"
        ))
    })
}

/// The algorithm `$algo` names, which must be cryptographic: HMAC and the
/// key derivations need a block size.
pub fn cryptographic_algorithm(name: &[u8], function: &str) -> Result<Algorithm, NativeError> {
    match Algorithm::find(name) {
        Some(algorithm) if algorithm.block_size().is_some() => Ok(algorithm),
        _ => Err(NativeError::value_error(format!(
            "{function}(): Argument #1 ($algo) must be a valid cryptographic hashing algorithm"
        ))),
    }
}

impl Options {
    /// Reads the `$options` argument, if it was passed, the way PHP does
    /// for `algorithm`, with PHP's deprecations for a seed that is not an
    /// int and a secret that is not a string.
    pub fn read(
        context: &mut dyn Context,
        algorithm: Algorithm,
        options: Option<&Value>,
        function: &str,
    ) -> Result<Options, NativeError> {
        let mut result = Options::default();
        let Some(Value::Array(options)) = options else {
            return Ok(result);
        };
        if !algorithm.takes_seed() {
            return Ok(result);
        }
        let seed = options.get(&ArrayKey::str("seed")).map(Value::deref);
        let secret = options.get(&ArrayKey::str("secret")).map(Value::deref);
        let secret_name = algorithm.secret_name();
        if let Some(name) = secret_name
            && seed.is_some()
            && secret.is_some()
        {
            return Err(NativeError::error(format!(
                "{name}: Only one of seed or secret is to be passed for initialization"
            )));
        }
        if let Some(seed) = seed {
            match &*seed {
                Value::Int(seed) => result.seed = *seed as u64,
                _ => context.deprecated(format!(
                    "{function}(): Passing a seed of a type other than int is deprecated because it is {}",
                    if secret_name.is_some() {
                        "ignored"
                    } else {
                        "the same as setting the seed to 0"
                    }
                )),
            }
        }
        if let Some(name) = secret_name
            && let Some(secret) = secret
        {
            result.secret = Some(read_secret(context, name, &secret, function)?);
        }
        Ok(result)
    }
}

/// An xxh3 or xxh128 secret: converted to a string, at least 136 bytes,
/// and cut to 256.
fn read_secret(
    context: &mut dyn Context,
    name: &str,
    secret: &Value,
    function: &str,
) -> Result<Vec<u8>, NativeError> {
    if !secret.is_string() {
        context.deprecated(format!(
            "{function}(): Passing a secret of a type other than string is deprecated because it \
             implicitly converts to a string, potentially hiding bugs"
        ));
    }
    let secret = to_php_string(secret, context.diagnostics())?;
    let mut bytes = secret.as_bytes().to_vec();
    if bytes.len() < MIN_SECRET {
        return Err(NativeError::error(format!(
            "{name}: Secret length must be >= {MIN_SECRET} bytes, {} bytes passed",
            bytes.len()
        )));
    }
    if bytes.len() > MAX_SECRET {
        context.warning(format!(
            "{function}(): {name}: Secret content exceeding {MAX_SECRET} bytes discarded"
        ));
        bytes.truncate(MAX_SECRET);
    }
    Ok(bytes)
}
