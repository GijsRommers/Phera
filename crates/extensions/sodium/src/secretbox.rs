//! `sodium_crypto_secretbox($message, $nonce, $key)` prepends a NaCl MAC.

use crypto_secretbox::{
    XSalsa20Poly1305,
    aead::{Aead, KeyInit},
};
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use crate::{exception, fixed, keygen};

fn encrypt(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    process(arguments, "sodium_crypto_secretbox", false)
}

fn decrypt(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    process(arguments, "sodium_crypto_secretbox_open", true)
}

fn process(arguments: &[Value], function: &str, decrypt: bool) -> NativeResult {
    let nonce = fixed::<24>(
        args::bytes(arguments, 1),
        function,
        2,
        "nonce",
        "SODIUM_CRYPTO_SECRETBOX_NONCEBYTES",
    )?;
    let key = fixed::<32>(
        args::bytes(arguments, 2),
        function,
        3,
        "key",
        "SODIUM_CRYPTO_SECRETBOX_KEYBYTES",
    )?;
    let cipher = XSalsa20Poly1305::new(key.into());
    let input = args::bytes(arguments, 0);
    if decrypt {
        Ok(cipher
            .decrypt(nonce.into(), input)
            .map_or(Value::Bool(false), Value::from))
    } else {
        cipher
            .encrypt(nonce.into(), input)
            .map(Value::from)
            .map_err(|_| exception("internal error"))
    }
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_secretbox",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            encrypt,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_secretbox_open",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            decrypt,
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync("sodium_crypto_secretbox_keygen", &[], keygen::<32>),
        "string",
    ),
];
