//! PHP's Sodium extension, for example `sodium_bin2hex("\\x00\\xff")` returns `'00ff'`.
//!
//! Generic hash and secretstream states are opaque PHP strings. Their length
//! matches PHP's, and scripts can copy them and resume each copy independently.
//! Secretstream headers and ciphertext interoperate with PHP.
//!
//! The `sodium_crypto_aead_aegis128l_*` and `sodium_crypto_aead_aegis256_*`
//! functions (encrypt, decrypt and keygen) are absent pending a published audit
//! of a pure Rust backend.
//! `sodium_crypto_sign` and `sodium_crypto_sign_detached` reject secret keys
//! whose stored public half dalek cannot decode, a deliberate difference from PHP.

use phera_extension_api::{
    ClassEntry, ClassHooks, Extension, FunctionEntry, NativeError, Registry,
};

mod aead;
mod blake2b_compress;
mod blake_state;
mod box_crypto;
mod bytes;
mod constants;
mod encoding;
mod hash;
mod key_exchange;
mod legacy_aead;
mod opaque_state;
mod password;
mod ristretto;
mod ristretto_scalar;
mod scrypt_password;
mod secretbox;
mod secretstream;
mod secretstream_state;
mod sign;
mod stream;
mod streaming_hash;
mod x25519;

pub struct Sodium;

impl Extension for Sodium {
    fn name(&self) -> &'static str {
        "sodium"
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            parent: Some("Exception"),
            hooks: Some(&ClassHooks {
                redact_trace_arguments: true,
                ..ClassHooks::EMPTY
            }),
            ..ClassEntry::class("SodiumException")
        });
        register(registry, encoding::FUNCTIONS);
        register(registry, ristretto_scalar::FUNCTIONS);
        register(registry, ristretto::FUNCTIONS);
        register(registry, bytes::FUNCTIONS);
        register(registry, stream::FUNCTIONS);
        register(registry, aead::FUNCTIONS);
        register(registry, password::FUNCTIONS);
        register(registry, scrypt_password::FUNCTIONS);
        register(registry, key_exchange::FUNCTIONS);
        register(registry, hash::FUNCTIONS);
        register(registry, streaming_hash::FUNCTIONS);
        register(registry, x25519::FUNCTIONS);
        register(registry, sign::FUNCTIONS);
        register(registry, secretbox::FUNCTIONS);
        register(registry, box_crypto::FUNCTIONS);
        register(registry, secretstream::FUNCTIONS);
        constants::register(registry);
    }
}

fn register(registry: &mut Registry, functions: &[(FunctionEntry, &'static str)]) {
    for &(entry, return_type) in functions {
        registry.function(entry);
        registry.describe_function(entry.name, return_type, None);
        if !entry.params.is_empty() && entry.name != "sodium_crypto_pwhash_str_needs_rehash" {
            registry.redact_argument_error_trace(entry.name);
        }
    }
}

fn exception(message: impl Into<String>) -> NativeError {
    NativeError::throw("SodiumException", message)
}

fn argument(function: &str, position: usize, name: &str, reason: &str) -> NativeError {
    exception(format!(
        "{function}(): Argument #{position} (${name}) {reason}"
    ))
}

fn fixed<'a, const N: usize>(
    bytes: &'a [u8],
    function: &str,
    position: usize,
    name: &str,
    constant: &str,
) -> Result<&'a [u8; N], NativeError> {
    bytes.try_into().map_err(|_| {
        argument(
            function,
            position,
            name,
            &format!("must be {constant} bytes long"),
        )
    })
}

fn random<const N: usize>() -> Result<zeroize::Zeroizing<[u8; N]>, NativeError> {
    let mut bytes = zeroize::Zeroizing::new([0; N]);
    getrandom::fill(&mut *bytes).map_err(|_| exception("internal error"))?;
    Ok(bytes)
}

fn keygen<const N: usize>(
    _: &mut dyn phera_extension_api::Context,
    _: &[phera_extension_api::Value],
) -> phera_extension_api::NativeResult {
    Ok(phera_extension_api::Value::from(random::<N>()?.as_slice()))
}

type KeyPair<'a> = (&'a [u8; 32], &'a [u8; 32]);

fn split_pair(pair: &[u8; 64]) -> Result<KeyPair<'_>, NativeError> {
    let (secret, public) = pair.split_at(32);
    Ok((
        secret.try_into().map_err(|_| exception("internal error"))?,
        public.try_into().map_err(|_| exception("internal error"))?,
    ))
}

fn list<const N: usize>(
    values: [phera_extension_api::Value; N],
) -> phera_extension_api::NativeResult {
    let mut array = phera_extension_api::PhpArray::new();
    for value in values {
        array.push(value).map_err(|_| exception("internal error"))?;
    }
    Ok(phera_extension_api::Value::from(array))
}

fn zeroed(length: usize) -> Result<Vec<u8>, NativeError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| exception("arithmetic overflow"))?;
    output.resize(length, 0);
    Ok(output)
}
