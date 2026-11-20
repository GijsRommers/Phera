//! `sodium_crypto_sign_detached($message, $secret_key)` returns a 64-byte Ed25519 signature.

use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, hazmat};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::{exception, fixed, random};

fn make_pair(seed: &[u8; 32]) -> Value {
    let key = SigningKey::from_bytes(seed);
    let secret = zeroize::Zeroizing::new(key.to_keypair_bytes());
    Value::from([secret.as_slice(), key.verifying_key().as_bytes().as_slice()].concat())
}

fn keypair(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(make_pair(&*random::<32>()?))
}

fn seed_keypair(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let seed = fixed::<32>(
        args::bytes(args, 0),
        "sodium_crypto_sign_seed_keypair",
        1,
        "seed",
        "SODIUM_CRYPTO_SIGN_SEEDBYTES",
    )?;
    Ok(make_pair(seed))
}

fn combine(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_sign_keypair_from_secretkey_and_publickey";
    let secret = secret(args, function, 0)?;
    let public = public(args, function, 1)?;
    Ok(Value::from([secret.as_slice(), public.as_slice()].concat()))
}

fn keypair_bytes<'a>(args: &'a [Value], function: &str) -> Result<&'a [u8; 96], NativeError> {
    fixed(
        args::bytes(args, 0),
        function,
        1,
        "key_pair",
        "SODIUM_CRYPTO_SIGN_KEYPAIRBYTES",
    )
}

fn secret_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    Ok(Value::from(
        &keypair_bytes(args, "sodium_crypto_sign_secretkey")?[..64],
    ))
}

fn public_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    Ok(Value::from(
        &keypair_bytes(args, "sodium_crypto_sign_publickey")?[64..],
    ))
}

fn derive_public(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    Ok(Value::from(
        &secret(args, "sodium_crypto_sign_publickey_from_secretkey", 0)?[32..],
    ))
}

fn secret<'a>(
    args: &'a [Value],
    function: &str,
    offset: usize,
) -> Result<&'a [u8; 64], NativeError> {
    fixed(
        args::bytes(args, offset),
        function,
        offset + 1,
        "secret_key",
        "SODIUM_CRYPTO_SIGN_SECRETKEYBYTES",
    )
}

fn public<'a>(
    args: &'a [Value],
    function: &str,
    offset: usize,
) -> Result<&'a [u8; 32], NativeError> {
    fixed(
        args::bytes(args, offset),
        function,
        offset + 1,
        "public_key",
        "SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES",
    )
}

fn signature(args: &[Value], function: &str) -> Result<Signature, NativeError> {
    let (seed, embedded_public) = crate::split_pair(secret(args, function, 1)?)?;
    let public = VerifyingKey::from_bytes(embedded_public).map_err(|_| {
        exception(if function == "sodium_crypto_sign_detached" {
            "signature creation failed"
        } else {
            "internal error"
        })
    })?;
    let expanded = hazmat::ExpandedSecretKey::from(seed);
    Ok(hazmat::raw_sign::<ed25519_dalek::Sha512>(
        &expanded,
        args::bytes(args, 0),
        &public,
    ))
}

fn detached(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let signature = signature(args, "sodium_crypto_sign_detached")?;
    Ok(Value::from(signature.to_bytes().as_slice()))
}

fn attached(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let signature = signature(args, "sodium_crypto_sign")?;
    Ok(Value::from(
        [signature.to_bytes().as_slice(), args::bytes(args, 0)].concat(),
    ))
}

fn valid(public: &[u8; 32], signature: &[u8; 64], message: &[u8]) -> bool {
    VerifyingKey::from_bytes(public).is_ok_and(|key| {
        key.verify_strict(message, &Signature::from_bytes(signature))
            .is_ok()
    })
}

fn verify(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_sign_verify_detached";
    let signature = fixed::<64>(
        args::bytes(args, 0),
        function,
        1,
        "signature",
        "SODIUM_CRYPTO_SIGN_BYTES",
    )?;
    Ok(Value::Bool(valid(
        public(args, function, 2)?,
        signature,
        args::bytes(args, 1),
    )))
}

fn open(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let public = public(args, "sodium_crypto_sign_open", 1)?;
    let Some((signature, message)) = args::bytes(args, 0).split_first_chunk::<64>() else {
        return Ok(Value::Bool(false));
    };
    Ok(if valid(public, signature, message) {
        Value::from(message)
    } else {
        Value::Bool(false)
    })
}

fn convert_secret(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let (seed, _) = crate::split_pair(secret(
        args,
        "sodium_crypto_sign_ed25519_sk_to_curve25519",
        0,
    )?)?;
    let key = SigningKey::from_bytes(seed);
    let scalar = zeroize::Zeroizing::new(curve25519_dalek::scalar::clamp_integer(
        key.to_scalar_bytes(),
    ));
    Ok(Value::from(scalar.as_slice()))
}

fn convert_public(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let public = public(args, "sodium_crypto_sign_ed25519_pk_to_curve25519", 0)?;
    let point = CompressedEdwardsY(*public)
        .decompress()
        .filter(|point| {
            !point.is_small_order()
                && point.is_torsion_free()
                && point.compress().as_bytes() == public
        })
        .ok_or_else(|| exception("conversion failed"))?;
    Ok(Value::from(point.to_montgomery().as_bytes().as_slice()))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_sign",
            &[
                Param::new("message", ParamType::String),
                Param::new("secret_key", ParamType::String).sensitive(),
            ],
            attached,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_detached",
            &[
                Param::new("message", ParamType::String),
                Param::new("secret_key", ParamType::String).sensitive(),
            ],
            detached,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_verify_detached",
            &[
                Param::new("signature", ParamType::String),
                Param::new("message", ParamType::String),
                Param::new("public_key", ParamType::String),
            ],
            verify,
        ),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_open",
            &[
                Param::new("signed_message", ParamType::String),
                Param::new("public_key", ParamType::String),
            ],
            open,
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync("sodium_crypto_sign_keypair", &[], keypair),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_seed_keypair",
            &[Param::new("seed", ParamType::String).sensitive()],
            seed_keypair,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_keypair_from_secretkey_and_publickey",
            &[
                Param::new("secret_key", ParamType::String).sensitive(),
                Param::new("public_key", ParamType::String),
            ],
            combine,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_publickey_from_secretkey",
            &[Param::new("secret_key", ParamType::String).sensitive()],
            derive_public,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_secretkey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            secret_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_publickey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            public_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_ed25519_sk_to_curve25519",
            &[Param::new("secret_key", ParamType::String).sensitive()],
            convert_secret,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_sign_ed25519_pk_to_curve25519",
            &[Param::new("public_key", ParamType::String)],
            convert_public,
        ),
        "string",
    ),
];
