//! `sodium_crypto_kx_client_session_keys($client, $server_pk)` gives receive and transmit keys.

use blake2b_simd::Params;
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use crate::{exception, fixed, list, random, split_pair, x25519};

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
        "sodium_crypto_kx_seed_keypair",
        1,
        "seed",
        "SODIUM_CRYPTO_KX_SEEDBYTES",
    )?;
    let digest = Params::new().hash_length(32).hash(seed);
    let secret = digest
        .as_bytes()
        .try_into()
        .map_err(|_| exception("internal error"))?;
    Ok(make_pair(secret))
}

fn secret_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let pair = fixed::<64>(
        args::bytes(args, 0),
        "sodium_crypto_kx_secretkey",
        1,
        "key_pair",
        "SODIUM_CRYPTO_KX_KEYPAIRBYTES",
    )?;
    Ok(Value::from(&pair[..32]))
}

fn public_key(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let pair = fixed::<64>(
        args::bytes(args, 0),
        "sodium_crypto_kx_publickey",
        1,
        "key_pair",
        "SODIUM_CRYPTO_KX_KEYPAIRBYTES",
    )?;
    Ok(Value::from(&pair[32..]))
}

enum Side {
    Client,
    Server,
}

fn session(args: &[Value], side: Side) -> NativeResult {
    let (function, own_name, peer_name) = match side {
        Side::Client => (
            "sodium_crypto_kx_client_session_keys",
            "client_key_pair",
            "server_key",
        ),
        Side::Server => (
            "sodium_crypto_kx_server_session_keys",
            "server_key_pair",
            "client_key",
        ),
    };
    let pair = fixed::<64>(
        args::bytes(args, 0),
        function,
        1,
        own_name,
        "SODIUM_CRYPTO_KX_KEYPAIRBYTES",
    )?;
    let (secret, public) = split_pair(pair)?;
    let peer = fixed::<32>(
        args::bytes(args, 1),
        function,
        2,
        peer_name,
        "SODIUM_CRYPTO_KX_PUBLICKEYBYTES",
    )?;
    let shared = x25519::shared_secret(secret, peer).ok_or_else(|| exception("internal error"))?;
    let (client, server) = match side {
        Side::Client => (public, peer),
        Side::Server => (peer, public),
    };
    let hash = Params::new()
        .to_state()
        .update(shared.as_bytes())
        .update(client)
        .update(server)
        .finalize();
    let (first, second) = hash.as_bytes().split_at(32);
    let (rx, tx) = match side {
        Side::Client => (first, second),
        Side::Server => (second, first),
    };
    list([Value::from(rx), Value::from(tx)])
}

fn client(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    session(args, Side::Client)
}

fn server(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    session(args, Side::Server)
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync("sodium_crypto_kx_keypair", &[], keypair),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kx_seed_keypair",
            &[Param::new("seed", ParamType::String).sensitive()],
            seed_keypair,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kx_secretkey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            secret_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kx_publickey",
            &[Param::new("key_pair", ParamType::String).sensitive()],
            public_key,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kx_client_session_keys",
            &[
                Param::new("client_key_pair", ParamType::String).sensitive(),
                Param::new("server_key", ParamType::String),
            ],
            client,
        ),
        "array",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_kx_server_session_keys",
            &[
                Param::new("server_key_pair", ParamType::String).sensitive(),
                Param::new("client_key", ParamType::String),
            ],
            server,
        ),
        "array",
    ),
];
