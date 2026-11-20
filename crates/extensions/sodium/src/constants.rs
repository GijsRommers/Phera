//! `SODIUM_CRYPTO_SECRETBOX_KEYBYTES` tells callers how many key bytes to supply.

use phera_extension_api::{ConstantValue, Registry};

pub fn register(registry: &mut Registry) {
    for &(name, value) in CONSTANTS {
        registry.constant(name, value);
    }
}

const CONSTANTS: &[(&str, ConstantValue)] = &[
    ("SODIUM_LIBRARY_VERSION", ConstantValue::String("1.0.22")),
    ("SODIUM_LIBRARY_MAJOR_VERSION", ConstantValue::Int(26)),
    ("SODIUM_LIBRARY_MINOR_VERSION", ConstantValue::Int(4)),
    (
        "SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AES256GCM_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES",
        ConstantValue::Int(12),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AES256GCM_ABYTES",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS128L_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS128L_ABYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS256_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES",
        ConstantValue::Int(32),
    ),
    ("SODIUM_CRYPTO_AEAD_AEGIS256_ABYTES", ConstantValue::Int(32)),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES",
        ConstantValue::Int(8),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_ABYTES",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES",
        ConstantValue::Int(12),
    ),
    (
        "SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_ABYTES",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NSECBYTES",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES",
        ConstantValue::Int(24),
    ),
    (
        "SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_ABYTES",
        ConstantValue::Int(16),
    ),
    ("SODIUM_CRYPTO_AUTH_BYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_AUTH_KEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_BOX_SEALBYTES", ConstantValue::Int(48)),
    ("SODIUM_CRYPTO_BOX_SECRETKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_BOX_PUBLICKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_BOX_KEYPAIRBYTES", ConstantValue::Int(64)),
    ("SODIUM_CRYPTO_BOX_MACBYTES", ConstantValue::Int(16)),
    ("SODIUM_CRYPTO_BOX_NONCEBYTES", ConstantValue::Int(24)),
    ("SODIUM_CRYPTO_BOX_SEEDBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KDF_BYTES_MIN", ConstantValue::Int(16)),
    ("SODIUM_CRYPTO_KDF_BYTES_MAX", ConstantValue::Int(64)),
    ("SODIUM_CRYPTO_KDF_CONTEXTBYTES", ConstantValue::Int(8)),
    ("SODIUM_CRYPTO_KDF_KEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KX_SEEDBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KX_SESSIONKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KX_PUBLICKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KX_SECRETKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_KX_KEYPAIRBYTES", ConstantValue::Int(64)),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_ABYTES",
        ConstantValue::Int(17),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES",
        ConstantValue::Int(24),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_MESSAGEBYTES_MAX",
        ConstantValue::Int(274877906816),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE",
        ConstantValue::Int(0),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_PUSH",
        ConstantValue::Int(1),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_REKEY",
        ConstantValue::Int(2),
    ),
    (
        "SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL",
        ConstantValue::Int(3),
    ),
    ("SODIUM_CRYPTO_GENERICHASH_BYTES", ConstantValue::Int(32)),
    (
        "SODIUM_CRYPTO_GENERICHASH_BYTES_MIN",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_GENERICHASH_BYTES_MAX",
        ConstantValue::Int(64),
    ),
    ("SODIUM_CRYPTO_GENERICHASH_KEYBYTES", ConstantValue::Int(32)),
    (
        "SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MIN",
        ConstantValue::Int(16),
    ),
    (
        "SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MAX",
        ConstantValue::Int(64),
    ),
    ("SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13", ConstantValue::Int(1)),
    ("SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13", ConstantValue::Int(2)),
    ("SODIUM_CRYPTO_PWHASH_ALG_DEFAULT", ConstantValue::Int(2)),
    ("SODIUM_CRYPTO_PWHASH_SALTBYTES", ConstantValue::Int(16)),
    (
        "SODIUM_CRYPTO_PWHASH_STRPREFIX",
        ConstantValue::String("$argon2id$"),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE",
        ConstantValue::Int(2),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE",
        ConstantValue::Int(67108864),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_OPSLIMIT_MODERATE",
        ConstantValue::Int(3),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_MEMLIMIT_MODERATE",
        ConstantValue::Int(268435456),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_OPSLIMIT_SENSITIVE",
        ConstantValue::Int(4),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_MEMLIMIT_SENSITIVE",
        ConstantValue::Int(1073741824),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRPREFIX",
        ConstantValue::String("$7$"),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE",
        ConstantValue::Int(524288),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE",
        ConstantValue::Int(16777216),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_SENSITIVE",
        ConstantValue::Int(33554432),
    ),
    (
        "SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_SENSITIVE",
        ConstantValue::Int(1073741824),
    ),
    ("SODIUM_CRYPTO_SCALARMULT_BYTES", ConstantValue::Int(32)),
    (
        "SODIUM_CRYPTO_SCALARMULT_SCALARBYTES",
        ConstantValue::Int(32),
    ),
    ("SODIUM_CRYPTO_SHORTHASH_BYTES", ConstantValue::Int(8)),
    ("SODIUM_CRYPTO_SHORTHASH_KEYBYTES", ConstantValue::Int(16)),
    ("SODIUM_CRYPTO_SECRETBOX_KEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_SECRETBOX_MACBYTES", ConstantValue::Int(16)),
    ("SODIUM_CRYPTO_SECRETBOX_NONCEBYTES", ConstantValue::Int(24)),
    ("SODIUM_CRYPTO_SIGN_BYTES", ConstantValue::Int(64)),
    ("SODIUM_CRYPTO_SIGN_SEEDBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES", ConstantValue::Int(32)),
    ("SODIUM_CRYPTO_SIGN_SECRETKEYBYTES", ConstantValue::Int(64)),
    ("SODIUM_CRYPTO_SIGN_KEYPAIRBYTES", ConstantValue::Int(96)),
    ("SODIUM_CRYPTO_STREAM_NONCEBYTES", ConstantValue::Int(24)),
    ("SODIUM_CRYPTO_STREAM_KEYBYTES", ConstantValue::Int(32)),
    (
        "SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES",
        ConstantValue::Int(24),
    ),
    (
        "SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES",
        ConstantValue::Int(32),
    ),
    ("SODIUM_BASE64_VARIANT_ORIGINAL", ConstantValue::Int(1)),
    (
        "SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING",
        ConstantValue::Int(3),
    ),
    ("SODIUM_BASE64_VARIANT_URLSAFE", ConstantValue::Int(5)),
    (
        "SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING",
        ConstantValue::Int(7),
    ),
    (
        "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES",
        ConstantValue::Int(64),
    ),
    (
        "SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES",
        ConstantValue::Int(32),
    ),
    (
        "SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES",
        ConstantValue::Int(64),
    ),
];
