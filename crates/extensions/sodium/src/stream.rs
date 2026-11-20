//! `sodium_crypto_stream_xor($message, $nonce, $key)` applies the XSalsa20 keystream.

use chacha20::{
    ChaCha20Legacy, ChaCha20LegacyCore, R20,
    cipher::{Block, KeyIvInit, StreamCipher, StreamCipherCore, StreamCipherSeek},
    hchacha,
};
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};
use salsa20::XSalsa20;
use zeroize::Zeroize;

use crate::{argument, exception, fixed, keygen, zeroed};

enum Stream {
    Salsa,
    ChaCha,
}

fn generate(args: &[Value], function: &str, stream: Stream) -> NativeResult {
    let length = args::int(args, 0).unwrap_or_default();
    if length <= 0 {
        return Err(argument(function, 1, "length", "must be greater than 0"));
    }
    apply(args, function, stream, Some(length as usize), None)
}

fn apply(
    args: &[Value],
    function: &str,
    stream: Stream,
    output_length: Option<usize>,
    initial_counter: Option<u64>,
) -> NativeResult {
    let (nonce_constant, key_constant) = match stream {
        Stream::Salsa => (
            "SODIUM_CRYPTO_STREAM_NONCEBYTES",
            "SODIUM_CRYPTO_STREAM_KEYBYTES",
        ),
        Stream::ChaCha => (
            "SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES",
            "SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES",
        ),
    };
    let key_index = if initial_counter.is_some() { 3 } else { 2 };
    let nonce = fixed::<24>(args::bytes(args, 1), function, 2, "nonce", nonce_constant)?;
    let key = fixed::<32>(
        args::bytes(args, key_index),
        function,
        key_index + 1,
        "key",
        key_constant,
    )?;
    let mut output = match output_length {
        Some(length) => zeroed(length)?,
        None => args::bytes(args, 0).to_vec(),
    };
    match stream {
        Stream::Salsa => XSalsa20::new(key.into(), nonce.into())
            .try_apply_keystream(&mut output)
            .map_err(|_| exception("internal error"))?,
        Stream::ChaCha => {
            let (prefix, suffix) = nonce.split_at(16);
            let prefix = prefix.try_into().map_err(|_| exception("internal error"))?;
            let mut subkey = hchacha::<R20>(key.into(), prefix);
            let counter = initial_counter.unwrap_or(0);
            if u128::from(counter) + output.len().div_ceil(64) as u128 > u128::from(u64::MAX) {
                chacha_wrapping(&subkey, suffix, counter, &mut output)?;
            } else {
                let mut cipher = ChaCha20Legacy::new_from_slices(&subkey, suffix)
                    .map_err(|_| exception("internal error"))?;
                cipher
                    .try_seek(u128::from(counter) * 64)
                    .map_err(|_| exception("internal error"))?;
                cipher
                    .try_apply_keystream(&mut output)
                    .map_err(|_| exception("internal error"))?;
            }
            subkey.as_mut_slice().zeroize();
        }
    }
    Ok(Value::from(output))
}

// PHP's explicit initial counter is cast to u64 and the cipher wraps at its maximum.
fn chacha_wrapping(
    key: &[u8],
    nonce: &[u8],
    counter: u64,
    output: &mut [u8],
) -> Result<(), phera_extension_api::NativeError> {
    let mut core =
        ChaCha20LegacyCore::new_from_slices(key, nonce).map_err(|_| exception("internal error"))?;
    core.set_block_pos(counter);
    for chunk in output.chunks_mut(64) {
        let mut block = Block::<ChaCha20LegacyCore>::default();
        block[..chunk.len()].copy_from_slice(chunk);
        core.apply_keystream_blocks(std::slice::from_mut(&mut block));
        chunk.copy_from_slice(&block[..chunk.len()]);
        block.as_mut_slice().zeroize();
    }
    Ok(())
}

fn salsa(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    generate(args, "sodium_crypto_stream", Stream::Salsa)
}

fn salsa_xor(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    apply(args, "sodium_crypto_stream_xor", Stream::Salsa, None, None)
}

fn chacha(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    generate(args, "sodium_crypto_stream_xchacha20", Stream::ChaCha)
}

fn chacha_xor(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    apply(
        args,
        "sodium_crypto_stream_xchacha20_xor",
        Stream::ChaCha,
        None,
        None,
    )
}

fn chacha_xor_ic(_: &mut dyn Context, args: &[Value]) -> NativeResult {
    let counter = args::int(args, 2).unwrap_or_default() as u64;
    apply(
        args,
        "sodium_crypto_stream_xchacha20_xor_ic",
        Stream::ChaCha,
        None,
        Some(counter),
    )
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "sodium_crypto_stream",
            &[
                Param::new("length", ParamType::Int),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            salsa,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_stream_xor",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            salsa_xor,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("sodium_crypto_stream_keygen", &[], keygen::<32>),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_stream_xchacha20",
            &[
                Param::new("length", ParamType::Int),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            chacha,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_stream_xchacha20_xor",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            chacha_xor,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_stream_xchacha20_xor_ic",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("nonce", ParamType::String),
                Param::new("counter", ParamType::Int),
                Param::new("key", ParamType::String).sensitive(),
            ],
            chacha_xor_ic,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("sodium_crypto_stream_xchacha20_keygen", &[], keygen::<32>),
        "string",
    ),
];
