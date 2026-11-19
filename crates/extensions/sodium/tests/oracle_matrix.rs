//! PHP and Phera both run `sodium_crypto_secretbox($bytes, $nonce, $key)` and its edge cases.

mod support;

use phera_extension_api::{Value, testing::oracle};
use support::{Matrix, repeated};

#[tokio::test]
async fn arity_and_utility_cases() {
    if oracle::find_php().is_none() {
        return;
    }
    let mut matrix = Matrix::new();
    let entries = matrix.registry.functions().cloned().collect::<Vec<_>>();
    for entry in entries {
        if entry.min_args() > 0 {
            matrix.case(entry.name, vec![]).await;
        }
        matrix
            .case(entry.name, vec![Value::Null; entry.params.len() + 1])
            .await;
    }
    for input in [
        "", "a", "\0", "g", "f f", "ff:00", "a1FF", ":00:", "00\0:ff", "a:b",
    ] {
        matrix
            .case("sodium_bin2hex", vec![Value::from(input)])
            .await;
        for ignore in ["", ":", " :", " :\0", "\0: "] {
            matrix
                .case(
                    "sodium_hex2bin",
                    vec![Value::from(input), Value::from(ignore)],
                )
                .await;
        }
    }
    for input in [
        "", "f", "ab", "Zg==", "Zg", "ZF==", "Zm9v===", "+w==", "-w", "Zg= =", "Zg== ",
    ] {
        for variant in [-1, 1, 2, 3, 5, 7, 4_294_967_297] {
            let params = vec![Value::from(input), Value::Int(variant)];
            matrix.case("sodium_bin2base64", params.clone()).await;
            for ignore in ["", "= ", " ", "="] {
                let mut decode = params.clone();
                decode.push(Value::from(ignore));
                matrix.case("sodium_base642bin", decode).await;
            }
        }
    }
    for first in [
        repeated(0xff, 17),
        repeated(0, 17),
        Value::from(""),
        Value::from("123"),
    ] {
        for second in [
            repeated(0xff, 17),
            repeated(0, 17),
            Value::from(""),
            Value::from("123"),
        ] {
            for function in ["sodium_add", "sodium_compare", "sodium_memcmp"] {
                matrix
                    .case(function, vec![first.clone(), second.clone()])
                    .await;
            }
        }
        matrix.case("sodium_increment", vec![first.clone()]).await;
        matrix.case("sodium_memzero", vec![first.clone()]).await;
        for block in [-1, 0, 1, 3, 8, 32] {
            matrix
                .case("sodium_pad", vec![first.clone(), Value::Int(block)])
                .await;
        }
    }
    matrix.compare("sodium_arity_and_utilities");
}

#[tokio::test]
async fn deterministic_crypto_cases() {
    if oracle::find_php().is_none() {
        return;
    }
    let mut matrix = Matrix::new();
    let message = Value::from(b"binary\x00\xffdata".as_slice());
    let key = repeated(7, 32);
    let nonce = repeated(1, 24);
    let aad = Value::from(b"header\x00\xc3".as_slice());
    let mut entries = vec![
        (
            "crypto_secretbox",
            vec![message.clone(), nonce.clone(), key.clone()],
        ),
        (
            "crypto_secretbox_open",
            vec![message.clone(), nonce.clone(), key.clone()],
        ),
        ("crypto_shorthash", vec![message.clone(), repeated(10, 16)]),
        ("crypto_auth", vec![message.clone(), key.clone()]),
        (
            "crypto_auth_verify",
            vec![repeated(0, 32), message.clone(), key.clone()],
        ),
        (
            "crypto_generichash",
            vec![message.clone(), key.clone(), Value::Int(32)],
        ),
        (
            "crypto_kdf_derive_from_key",
            vec![
                Value::Int(32),
                Value::Int(50),
                repeated(0xff, 8),
                key.clone(),
            ],
        ),
        ("crypto_scalarmult", vec![key.clone(), repeated(4, 32)]),
        ("crypto_scalarmult_base", vec![key.clone()]),
        (
            "crypto_stream_xor",
            vec![message.clone(), nonce.clone(), key.clone()],
        ),
        (
            "crypto_stream_xchacha20_xor",
            vec![message.clone(), nonce.clone(), key.clone()],
        ),
        ("crypto_box_seed_keypair", vec![key.clone()]),
        ("crypto_kx_seed_keypair", vec![key.clone()]),
        (
            "crypto_box_keypair_from_secretkey_and_publickey",
            vec![key.clone(), key.clone()],
        ),
        ("crypto_box_publickey_from_secretkey", vec![key.clone()]),
        ("crypto_sign_seed_keypair", vec![key.clone()]),
        ("crypto_sign_ed25519_pk_to_curve25519", vec![key.clone()]),
    ];
    let sign_key = binary(concat!(
        "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
    ));
    let pubkey = binary("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
    entries.extend([
        (
            "crypto_sign_ed25519_sk_to_curve25519",
            vec![sign_key.clone()],
        ),
        (
            "crypto_sign_publickey_from_secretkey",
            vec![sign_key.clone()],
        ),
        (
            "crypto_sign_keypair_from_secretkey_and_publickey",
            vec![sign_key.clone(), pubkey.clone()],
        ),
        (
            "crypto_sign_detached",
            vec![message.clone(), sign_key.clone()],
        ),
        ("crypto_sign", vec![message.clone(), sign_key]),
        ("crypto_sign_open", vec![message.clone(), pubkey.clone()]),
        (
            "crypto_sign_verify_detached",
            vec![repeated(0, 64), message.clone(), pubkey],
        ),
    ]);
    for (name, args) in entries {
        matrix.vary(&format!("sodium_{name}"), args).await;
    }
    for (algorithm, size) in [
        ("aes256gcm", 12),
        ("chacha20poly1305", 8),
        ("chacha20poly1305_ietf", 12),
        ("xchacha20poly1305_ietf", 24),
    ] {
        for operation in ["encrypt", "decrypt"] {
            let function = format!("sodium_crypto_aead_{algorithm}_{operation}");
            matrix
                .vary(
                    &function,
                    vec![message.clone(), aad.clone(), repeated(3, size), key.clone()],
                )
                .await;
        }
    }
    for length in [16, 24, 31, 32, 63, 64, 65, 128] {
        for size in [0, 15, 16, 31, 32, 64, 65] {
            matrix
                .case(
                    "sodium_crypto_generichash",
                    vec![message.clone(), repeated(8, size), Value::Int(length)],
                )
                .await;
        }
    }
    for counter in [0, 1, 65536, i64::from(u32::MAX), 1 << 32, i64::MAX, -2, -1] {
        matrix
            .case(
                "sodium_crypto_stream_xchacha20_xor_ic",
                vec![
                    message.clone(),
                    nonce.clone(),
                    Value::Int(counter),
                    key.clone(),
                ],
            )
            .await;
    }
    for length in [0, -1, 1, 16, 64, 65, 255] {
        for name in ["sodium_crypto_stream", "sodium_crypto_stream_xchacha20"] {
            matrix
                .case(name, vec![Value::Int(length), nonce.clone(), key.clone()])
                .await;
        }
    }
    for length in [0, 1, 15, 16, 17, 63, 64, 65, 127, 128, 129] {
        for key_size in [0, 16, 32, 64] {
            for output in [16, 32, 64] {
                matrix
                    .case(
                        "sodium_crypto_generichash",
                        vec![
                            repeated(0xfe, length),
                            repeated(0x31, key_size),
                            Value::Int(output),
                        ],
                    )
                    .await;
            }
        }
        for nonce_size in [0, 1, 23, 24, 25, 31, 32, 64] {
            matrix
                .case(
                    "sodium_crypto_secretbox",
                    vec![
                        repeated(0xff, length),
                        repeated(0x02, nonce_size),
                        key.clone(),
                    ],
                )
                .await;
        }
    }
    matrix.compare("sodium_deterministic_crypto");
}

fn binary(hexadecimal: &str) -> Value {
    Value::from(hex::decode(hexadecimal).unwrap())
}
