//! `sodium_crypto_box_seal($message, $public_key)` prepends an ephemeral public key.

use blake2b_simd::Params;
use crypto_box::{PublicKey, SalsaBox, SecretKey, aead::Aead};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};
use sha2::{Digest, Sha512};
use zeroize::Zeroizing;

use crate::{exception, fixed, random, x25519};

fn make_pair(secret: &[u8; 32]) -> Value {
    let public = x25519::public_key(secret);
    Value::from([secret.as_slice(), public.as_slice()].concat())
}

fn keypair(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(make_pair(&*random::<32>()?))
}

fn seed_keypair(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let seed = fixed::<32>(
        args::bytes(args, 0),
        "sodium_crypto_box_seed_keypair",
        1,
        "seed",
        "SODIUM_CRYPTO_BOX_SEEDBYTES",
    )?;
    let digest = Zeroizing::new(<[u8; 64]>::from(Sha512::digest(seed)));
    let (secret, _) = digest.split_at(32);
    let secret = secret.try_into().map_err(|_| exception("internal error"))?;
    Ok(make_pair(secret))
}

fn combine(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_box_keypair_from_secretkey_and_publickey";
    let secret = fixed::<32>(
        args::bytes(args, 0),
        function,
        1,
        "secret_key",
        "SODIUM_CRYPTO_BOX_SECRETKEYBYTES",
    )?;
    let public = fixed::<32>(
        args::bytes(args, 1),
        function,
        2,
        "public_key",
        "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES",
    )?;
    Ok(Value::from([secret.as_slice(), public.as_slice()].concat()))
}

fn secret_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let (secret, _) = read_pair(args, "sodium_crypto_box_secretkey", 0)?;
    Ok(Value::from(secret.as_slice()))
}

fn public_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let (_, public) = read_pair(args, "sodium_crypto_box_publickey", 0)?;
    Ok(Value::from(public.as_slice()))
}

fn derive_public(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let secret = fixed::<32>(
        args::bytes(args, 0),
        "sodium_crypto_box_publickey_from_secretkey",
        1,
        "secret_key",
        "SODIUM_CRYPTO_BOX_SECRETKEYBYTES",
    )?;
    Ok(Value::from(x25519::public_key(secret).as_slice()))
}

fn read_pair<'a>(
    args: &'a [Value],
    function: &str,
    offset: usize,
) -> Result<crate::KeyPair<'a>, NativeError> {
    let pair = fixed::<64>(
        args::bytes(args, offset),
        function,
        offset + 1,
        "key_pair",
        "SODIUM_CRYPTO_BOX_KEYPAIRBYTES",
    )?;
    crate::split_pair(pair)
}

fn cipher(secret: &[u8; 32], public: &[u8; 32]) -> Option<SalsaBox> {
    // RustCrypto's constructor accepts all Montgomery encodings. Sodium rejects
    // a non-contributory peer key before it reaches the NaCl construction.
    x25519::shared_secret(secret, public)?;
    Some(SalsaBox::new(
        &PublicKey::from(*public),
        &SecretKey::from(*secret),
    ))
}

fn encrypt(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_box";
    let nonce = nonce(args, function)?;
    let (secret, public) = read_pair(args, function, 2)?;
    cipher(secret, public)
        .and_then(|cipher| cipher.encrypt(nonce.into(), args::bytes(args, 0)).ok())
        .map(Value::from)
        .ok_or_else(|| exception("internal error"))
}

fn decrypt(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_box_open";
    let nonce = nonce(args, function)?;
    let (secret, public) = read_pair(args, function, 2)?;
    Ok(cipher(secret, public)
        .and_then(|cipher| cipher.decrypt(nonce.into(), args::bytes(args, 0)).ok())
        .map_or(Value::Bool(false), Value::from))
}

fn nonce<'a>(args: &'a [Value], function: &str) -> Result<&'a [u8; 24], NativeError> {
    fixed(
        args::bytes(args, 1),
        function,
        2,
        "nonce",
        "SODIUM_CRYPTO_BOX_NONCEBYTES",
    )
}

fn seal_nonce(ephemeral: &[u8; 32], recipient: &[u8; 32]) -> crypto_box::Nonce {
    let hash = Params::new()
        .hash_length(24)
        .to_state()
        .update(ephemeral)
        .update(recipient)
        .finalize();
    *crypto_box::Nonce::from_slice(hash.as_bytes())
}

fn seal(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let recipient = fixed::<32>(
        args::bytes(args, 1),
        "sodium_crypto_box_seal",
        2,
        "public_key",
        "SODIUM_CRYPTO_BOX_PUBLICKEYBYTES",
    )?;
    let secret = random::<32>()?;
    let ephemeral = x25519::public_key(&secret);
    let nonce = seal_nonce(&ephemeral, recipient);
    let ciphertext = cipher(&secret, recipient)
        .and_then(|cipher| cipher.encrypt(&nonce, args::bytes(args, 0)).ok())
        .ok_or_else(|| exception("internal error"))?;
    Ok(Value::from(
        [ephemeral.as_slice(), ciphertext.as_slice()].concat(),
    ))
}

fn seal_open(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let (secret, recipient) = read_pair(args, "sodium_crypto_box_seal_open", 1)?;
    let Some((ephemeral, ciphertext)) = args::bytes(args, 0).split_first_chunk::<32>() else {
        return Ok(Value::Bool(false));
    };
    let nonce = seal_nonce(ephemeral, recipient);
    Ok(cipher(secret, ephemeral)
        .and_then(|cipher| cipher.decrypt(&nonce, ciphertext).ok())
        .map_or(Value::Bool(false), Value::from))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_box",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("nonce", ParamType::String),
                Param::new("key_pair", ParamType::String).sensitive(),
            ],
            encrypt,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_open",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key_pair", ParamType::String).sensitive(),
            ],
            decrypt,
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync("sodium_crypto_box_keypair", &[], keypair),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_seed_keypair",
            &[Param::new("seed", ParamType::String).sensitive()],
            seed_keypair,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_keypair_from_secretkey_and_publickey",
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
            "sodium_crypto_box_secretkey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            secret_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_publickey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            public_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_publickey_from_secretkey",
            &[Param::new("secret_key", ParamType::String).sensitive()],
            derive_public,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_seal",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("public_key", ParamType::String),
            ],
            seal,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_box_seal_open",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("key_pair", ParamType::String).sensitive(),
            ],
            seal_open,
        ),
        "string|false",
    ),
];
