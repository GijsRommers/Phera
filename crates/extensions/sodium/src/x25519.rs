//! `sodium_crypto_scalarmult($secret, $peer)` derives an X25519 shared secret.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};
use x25519_dalek::{PublicKey, SharedSecret, StaticSecret};

use crate::{exception, fixed};

pub fn public_key(secret: &[u8; 32]) -> [u8; 32] {
    PublicKey::from(&StaticSecret::from(*secret)).to_bytes()
}

pub fn shared_secret(secret: &[u8; 32], peer: &[u8; 32]) -> Option<SharedSecret> {
    let shared = StaticSecret::from(*secret).diffie_hellman(&PublicKey::from(*peer));
    shared.was_contributory().then_some(shared)
}

fn scalarmult(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let function = "sodium_crypto_scalarmult";
    let secret = fixed::<32>(
        args::bytes(args, 0),
        function,
        1,
        "n",
        "SODIUM_CRYPTO_SCALARMULT_SCALARBYTES",
    )?;
    let peer = fixed::<32>(
        args::bytes(args, 1),
        function,
        2,
        "p",
        "SODIUM_CRYPTO_SCALARMULT_BYTES",
    )?;
    shared_secret(secret, peer)
        .map(|shared| Value::from(shared.as_bytes().as_slice()))
        .ok_or_else(|| exception("internal error"))
}

fn base(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let secret = fixed::<32>(
        args::bytes(args, 0),
        "sodium_crypto_scalarmult_base",
        1,
        "secret_key",
        "SODIUM_CRYPTO_BOX_SECRETKEYBYTES",
    )?;
    Ok(Value::from(public_key(secret).as_slice()))
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_scalarmult",
            &[
                Param::new("n", ParamType::String),
                Param::new("p", ParamType::String),
            ],
            scalarmult,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_scalarmult_base",
            &[Param::new("secret_key", ParamType::String).sensitive()],
            base,
        ),
        "string",
    ),
];
