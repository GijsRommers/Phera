//! `sodium_crypto_generichash($message, $key, 32)` computes keyed or unkeyed BLAKE2b.

use std::hash::Hasher;

use blake2b_simd::Params;
use hmac::{Hmac, Mac};
use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};
use sha2::Sha512;
use siphasher::sip::SipHasher24;

use crate::{argument, exception, fixed, keygen};

fn shorthash(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let key = fixed::<16>(
        args::bytes(args, 1),
        "sodium_crypto_shorthash",
        2,
        "key",
        "SODIUM_CRYPTO_SHORTHASH_KEYBYTES",
    )?;
    let mut hasher = SipHasher24::new_with_key(key);
    hasher.write(args::bytes(args, 0));
    Ok(Value::from(hasher.finish().to_le_bytes().as_slice()))
}

fn mac(key: &[u8; 32], message: &[u8]) -> Result<Hmac<Sha512>, NativeError> {
    let mut mac = Hmac::<Sha512>::new_from_slice(key).map_err(|_| exception("internal error"))?;
    mac.update(message);
    Ok(mac)
}

fn auth(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let key = fixed::<32>(
        args::bytes(args, 1),
        "sodium_crypto_auth",
        2,
        "key",
        "SODIUM_CRYPTO_AUTH_KEYBYTES",
    )?;
    let digest = mac(key, args::bytes(args, 0))?.finalize().into_bytes();
    Ok(Value::from(&digest[..32]))
}

fn verify(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_auth_verify";
    let key = fixed::<32>(
        args::bytes(args, 2),
        function,
        3,
        "key",
        "SODIUM_CRYPTO_AUTH_KEYBYTES",
    )?;
    let tag = fixed::<32>(
        args::bytes(args, 0),
        function,
        1,
        "mac",
        "SODIUM_CRYPTO_AUTH_BYTES",
    )?;
    Ok(Value::Bool(
        mac(key, args::bytes(args, 1))?
            .verify_truncated_left(tag)
            .is_ok(),
    ))
}

fn generic(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let length = hash_length(args, 2)?;
    let key = hash_key(args, 1)?;
    let digest = Params::new()
        .hash_length(length)
        .key(key)
        .hash(args::bytes(args, 0));
    Ok(Value::from(digest.as_bytes()))
}

pub(crate) fn hash_length(args: &[Value], index: usize) -> Result<usize, NativeError> {
    let length = args::int(args, index).unwrap_or(32);
    if !(16..=64).contains(&length) {
        return Err(exception("unsupported output length"));
    }
    Ok(length as usize)
}

pub(crate) fn hash_key(args: &[Value], index: usize) -> Result<&[u8], NativeError> {
    let key = args::bytes(args, index);
    if !key.is_empty() && !(16..=64).contains(&key.len()) {
        return Err(exception("unsupported key length"));
    }
    Ok(key)
}

fn derive(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_kdf_derive_from_key";
    let length = args::int(args, 0).unwrap_or_default();
    if length < 16 {
        return Err(argument(
            function,
            1,
            "subkey_length",
            "must be greater than or equal to SODIUM_CRYPTO_KDF_BYTES_MIN",
        ));
    }
    if length > 64 {
        return Err(argument(
            function,
            1,
            "subkey_length",
            "must be less than or equal to SODIUM_CRYPTO_KDF_BYTES_MAX",
        ));
    }
    let id = args::int(args, 1).unwrap_or_default();
    if id < 0 {
        return Err(argument(
            function,
            2,
            "subkey_id",
            "must be greater than or equal to 0",
        ));
    }
    let context = fixed::<8>(
        args::bytes(args, 2),
        function,
        3,
        "context",
        "SODIUM_CRYPTO_KDF_CONTEXTBYTES",
    )?;
    let key = fixed::<32>(
        args::bytes(args, 3),
        function,
        4,
        "key",
        "SODIUM_CRYPTO_KDF_KEYBYTES",
    )?;
    let digest = Params::new()
        .hash_length(length as usize)
        .personal(context)
        .salt(&id.to_le_bytes())
        .key(key)
        .hash(&[]);
    Ok(Value::from(digest.as_bytes()))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_shorthash",
            &[
                Param::new("message", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            shorthash,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("sodium_crypto_shorthash_keygen", &[], keygen::<16>),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_auth",
            &[
                Param::new("message", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            auth,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_auth_verify",
            &[
                Param::new("mac", ParamType::String),
                Param::new("message", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            verify,
        ),
        "bool",
    ),
    (
        FunctionEntry::sync("sodium_crypto_auth_keygen", &[], keygen::<32>),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_generichash",
            &[
                Param::new("message", ParamType::String),
                Param::new("key", ParamType::String)
                    .sensitive()
                    .with_default(ConstantValue::String("")),
                Param::new("length", ParamType::Int).with_constant_default(
                    "SODIUM_CRYPTO_GENERICHASH_BYTES",
                    ConstantValue::Int(32),
                ),
            ],
            generic,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("sodium_crypto_generichash_keygen", &[], keygen::<32>),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kdf_derive_from_key",
            &[
                Param::new("subkey_length", ParamType::Int),
                Param::new("subkey_id", ParamType::Int),
                Param::new("context", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            derive,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("sodium_crypto_kdf_keygen", &[], keygen::<32>),
        "string",
    ),
];
