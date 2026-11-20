//! `sodium_crypto_aead_chacha20poly1305_ietf_encrypt($message, $ad, $nonce, $key)`
//! authenticates the associated data alongside the ciphertext.

use aes_gcm::Aes256Gcm;
use chacha20poly1305::{
    ChaCha20Poly1305, XChaCha20Poly1305,
    aead::{Aead, KeyInit, Nonce, Payload},
};
use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use crate::{argument, exception, keygen, legacy_aead};

#[derive(Clone, Copy)]
enum Algorithm {
    Aes,
    ChaCha,
    ChaChaIetf,
    XChaCha,
}

impl Algorithm {
    fn prefix(self) -> &'static str {
        match self {
            Self::Aes => "aes256gcm",
            Self::ChaCha => "chacha20poly1305",
            Self::ChaChaIetf => "chacha20poly1305_ietf",
            Self::XChaCha => "xchacha20poly1305_ietf",
        }
    }

    fn nonce_size(self) -> usize {
        match self {
            Self::ChaCha => 8,
            Self::Aes | Self::ChaChaIetf => 12,
            Self::XChaCha => 24,
        }
    }

    fn max_message(self, operation: Operation) -> u64 {
        match self {
            Self::Aes => match operation {
                Operation::Encrypt => (1 << 36) - 48,
                Operation::Decrypt => (1 << 36) - 32,
            },
            Self::ChaChaIetf => u64::from(u32::MAX) * 64,
            Self::ChaCha | Self::XChaCha => u64::MAX - 16,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Operation {
    Encrypt,
    Decrypt,
}

fn apply(algorithm: Algorithm, operation: Operation, args: &[Value]) -> NativeResult {
    let input = args::bytes(args, 0);
    let aad = args::bytes(args, 1);
    let nonce = args::bytes(args, 2);
    let key = args::bytes(args, 3);
    if nonce.len() != algorithm.nonce_size() || key.len() != 32 {
        let (index, name, suffix) = if nonce.len() != algorithm.nonce_size() {
            (3, "nonce", "NPUBBYTES")
        } else {
            (4, "key", "KEYBYTES")
        };
        let method = match operation {
            Operation::Encrypt => "encrypt",
            Operation::Decrypt => "decrypt",
        };
        let prefix = algorithm.prefix();
        let function = format!("sodium_crypto_aead_{prefix}_{method}");
        let kind = prefix.to_ascii_uppercase();
        return Err(argument(
            &function,
            index,
            name,
            &format!("must be SODIUM_CRYPTO_AEAD_{kind}_{suffix} bytes long"),
        ));
    }
    let message_len = match operation {
        Operation::Encrypt => input.len(),
        Operation::Decrypt => {
            let Some(length) = input.len().checked_sub(16) else {
                return Ok(Value::Bool(false));
            };
            length
        }
    };
    if message_len as u64 > algorithm.max_message(operation) {
        if matches!((operation, algorithm), (Operation::Decrypt, Algorithm::Aes)) {
            return Err(argument(
                "sodium_crypto_aead_aes256gcm_decrypt",
                1,
                "ciphertext",
                "is too long",
            ));
        }
        return Err(exception("message too long for a single key"));
    }
    let output = match algorithm {
        Algorithm::Aes if aes_gcm_available() => {
            crypt::<Aes256Gcm>(operation, key, nonce, input, aad)
        }
        Algorithm::Aes => None,
        Algorithm::ChaChaIetf => crypt::<ChaCha20Poly1305>(operation, key, nonce, input, aad),
        Algorithm::XChaCha => crypt::<XChaCha20Poly1305>(operation, key, nonce, input, aad),
        Algorithm::ChaCha => legacy_aead::crypt(operation, key, nonce, input, aad),
    };
    match (operation, output) {
        (_, Some(bytes)) => Ok(Value::from(bytes)),
        (Operation::Decrypt, None) => Ok(Value::Bool(false)),
        (Operation::Encrypt, None) => Err(exception("internal error")),
    }
}

fn crypt<A: Aead + KeyInit>(
    operation: Operation,
    key: &[u8],
    nonce: &[u8],
    input: &[u8],
    aad: &[u8],
) -> Option<Vec<u8>> {
    let cipher = A::new_from_slice(key).ok()?;
    let nonce = <&Nonce<A>>::try_from(nonce).ok()?;
    let payload = Payload { msg: input, aad };
    match operation {
        Operation::Encrypt => cipher.encrypt(nonce, payload).ok(),
        Operation::Decrypt => cipher.decrypt(nonce, payload).ok(),
    }
}

fn available(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(aes_gcm_available()))
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn aes_gcm_available() -> bool {
    std::is_x86_feature_detected!("aes") && std::is_x86_feature_detected!("pclmulqdq")
}

#[cfg(target_arch = "aarch64")]
fn aes_gcm_available() -> bool {
    std::arch::is_aarch64_feature_detected!("aes")
        && std::arch::is_aarch64_feature_detected!("pmull")
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
fn aes_gcm_available() -> bool {
    false
}

pub const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync("sodium_crypto_aead_aes256gcm_is_available", &[], available),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_aes256gcm_encrypt",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::Aes, Operation::Encrypt, args),
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_aes256gcm_decrypt",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::Aes, Operation::Decrypt, args),
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync("sodium_crypto_aead_aes256gcm_keygen", &[], keygen::<32>),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_encrypt",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::ChaCha, Operation::Encrypt, args),
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_decrypt",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::ChaCha, Operation::Decrypt, args),
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_keygen",
            &[],
            keygen::<32>,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_ietf_encrypt",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::ChaChaIetf, Operation::Encrypt, args),
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_ietf_decrypt",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::ChaChaIetf, Operation::Decrypt, args),
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_chacha20poly1305_ietf_keygen",
            &[],
            keygen::<32>,
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_xchacha20poly1305_ietf_encrypt",
            &[
                Param::new("message", ParamType::String).sensitive(),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::XChaCha, Operation::Encrypt, args),
        ),
        "string",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_xchacha20poly1305_ietf_decrypt",
            &[
                Param::new("ciphertext", ParamType::String),
                Param::new("additional_data", ParamType::String),
                Param::new("nonce", ParamType::String),
                Param::new("key", ParamType::String).sensitive(),
            ],
            |_, args| apply(Algorithm::XChaCha, Operation::Decrypt, args),
        ),
        "string|false",
    ),
    (
        FunctionEntry::sync(
            "sodium_crypto_aead_xchacha20poly1305_ietf_keygen",
            &[],
            keygen::<32>,
        ),
        "string",
    ),
];
