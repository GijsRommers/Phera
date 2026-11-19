//! PHP 8.5 differential coverage for cipher output, warnings, and output references.

use phera_extension_api::{PhpArray, Value};

mod support;
use support::Matrix;

const CIPHERS: &[&str] = &[
    "aes-128-cbc",
    "aes-128-ccm",
    "aes-128-cfb",
    "aes-128-ctr",
    "aes-128-ecb",
    "aes-128-gcm",
    "aes-128-ofb",
    "aes-192-cbc",
    "aes-192-ccm",
    "aes-192-cfb",
    "aes-192-ctr",
    "aes-192-ecb",
    "aes-192-gcm",
    "aes-192-ofb",
    "aes-256-cbc",
    "aes-256-ccm",
    "aes-256-cfb",
    "aes-256-ctr",
    "aes-256-ecb",
    "aes-256-gcm",
    "aes-256-ofb",
    "chacha20-poly1305",
];

#[test]
fn symmetric_calls_match_php() {
    let mut matrix = Matrix::new();
    for &method in CIPHERS {
        let aead =
            method.ends_with("ccm") || method.ends_with("gcm") || method.starts_with("chacha");
        let iv_size = if aead {
            12
        } else if method.ends_with("ecb") {
            0
        } else {
            16
        };
        let key_size = if method.starts_with("aes-128") {
            16
        } else if method.starts_with("aes-192") {
            24
        } else {
            32
        };
        let iv_sizes = if aead {
            [0, iv_size, iv_size]
        } else {
            [0, iv_size, 17]
        };
        matrix.case("openssl_cipher_iv_length", vec![Value::from(method)], None);
        matrix.case("openssl_cipher_key_length", vec![Value::from(method)], None);
        for data in [
            b"".as_slice(),
            b"x",
            b"\0\xff",
            &[b'.'; 16],
            &[b'1'; 17],
            &[b'2'; 32],
        ] {
            for key in [0, 1, key_size, key_size + 1] {
                for iv in iv_sizes {
                    for flags in [0, 1, 2, 3, 4, 5, -1] {
                        let arguments = vec![
                            Value::from(data.to_vec()),
                            Value::from(method.to_ascii_uppercase()),
                            Value::from(vec![b'k'; key]),
                            Value::Int(flags),
                            Value::from(vec![b'i'; iv]),
                            Value::from("tag before call"),
                            Value::from(b"additional\0\xffdata".to_vec()),
                            Value::Int(16),
                        ];
                        let result = matrix.case("openssl_encrypt", arguments.clone(), Some(5));
                        if let Some((value @ Value::String(_), tag)) = result {
                            let mut decrypt = arguments;
                            decrypt.pop();
                            decrypt[0] = value;
                            decrypt[5] = tag;
                            matrix.case("openssl_decrypt", decrypt.clone(), None);
                            if aead {
                                decrypt[5] = Value::from([b'!'; 16].to_vec());
                            } else {
                                decrypt[0] = Value::from("garbage!!!");
                            }
                            matrix.case("openssl_decrypt", decrypt, None);
                        }
                    }
                }
            }
        }
    }
    for method in [
        "aes-128-gcm",
        "aes-192-ccm",
        "chacha20-poly1305",
        "aes-256-cbc",
    ] {
        for tag_size in [-1, 0, 1, 4, 5, 6, 8, 10, 12, 13, 14, 15, 16, 17, i64::MAX] {
            matrix.case(
                "openssl_encrypt",
                vec![
                    Value::from("message"),
                    Value::from(method),
                    Value::from("key"),
                    Value::Int(1),
                    Value::from("0123456789ab"),
                    Value::from("sentinel"),
                    Value::from("aad"),
                    Value::Int(tag_size),
                ],
                Some(5),
            );
        }
    }
    for method in [
        "",
        "fictitious",
        "AES128",
        "aes-256-cbc\0suffix",
        "\0aes-128-cbc",
    ] {
        let name = Value::from(method);
        for function in ["openssl_cipher_iv_length", "openssl_cipher_key_length"] {
            matrix.case(function, vec![name.clone()], None);
        }
        for function in ["openssl_encrypt", "openssl_decrypt"] {
            matrix.case(
                function,
                vec![Value::from("data"), name.clone(), Value::from("key")],
                None,
            );
        }
    }
    for function in ["openssl_encrypt", "openssl_decrypt"] {
        for invalid in [Value::Null, Value::from(PhpArray::new()), Value::Int(123)] {
            matrix.case(function, vec![invalid.clone()], None);
            matrix.case(
                function,
                vec![invalid.clone(), Value::from("aes-128-cbc"), invalid],
                None,
            );
        }
    }
    matrix.check("openssl_symmetric");
}

#[test]
fn digest_and_kdf_calls_match_php() {
    let mut matrix = Matrix::new();
    for method in [
        "",
        "fake",
        "sha1\0suffix",
        "SHA224",
        "sha256",
        "sha-256",
        "sha384",
        "sha512",
        "sha512-224",
        "sha512-256",
        "sha3-224",
        "sha3-256",
        "sha3-384",
        "sha3-512",
        "md5",
        "ripemd160",
    ] {
        for bytes in [b"".as_slice(), b"short", b"\0\xff", &[0xde; 64], &[0; 129]] {
            for raw in [false, true] {
                matrix.case(
                    "openssl_digest",
                    vec![
                        Value::from(bytes.to_vec()),
                        Value::from(method),
                        Value::Bool(raw),
                    ],
                    None,
                );
            }
            for length in [0, 1, 31, 64, 128, i64::MAX] {
                for rounds in [-1, 0, 1, 2, i64::MAX] {
                    matrix.case(
                        "openssl_pbkdf2",
                        vec![
                            Value::from(bytes.to_vec()),
                            Value::from(b"\0\xffsalt".to_vec()),
                            Value::Int(length),
                            Value::Int(rounds),
                            Value::from(method),
                        ],
                        None,
                    );
                }
            }
        }
    }
    for length in [0, -1, i64::MAX, i64::MIN] {
        matrix.case(
            "openssl_random_pseudo_bytes",
            vec![Value::Int(length), Value::Null],
            Some(1),
        );
    }
    matrix.case("openssl_error_string", vec![], None);
    matrix.check("openssl_digests");
}

#[test]
fn all_gcm_nonce_lengths_match_php() {
    let mut matrix = Matrix::new();
    for (method, key_length) in [
        ("aes-128-gcm", 16),
        ("aes-192-gcm", 24),
        ("aes-256-gcm", 32),
    ] {
        for length in 1..=128 {
            let nonce = (0..length).map(|byte| byte as u8).collect::<Vec<_>>();
            for tag_length in [4, 8, 12, 13, 14, 15, 16] {
                let mut arguments = vec![
                    Value::from("message"),
                    Value::from(method),
                    Value::from(vec![b'k'; key_length]),
                    Value::Int(1),
                    Value::from(nonce.clone()),
                    Value::from("sentinel"),
                    Value::from(b"\0\xff AAD".to_vec()),
                    Value::Int(tag_length),
                ];
                let (ciphertext, tag) = matrix
                    .case("openssl_encrypt", arguments.clone(), Some(5))
                    .expect("encrypt returns a result");
                assert!(matches!(ciphertext, Value::String(_)));
                arguments[0] = ciphertext;
                arguments[5] = tag;
                arguments.pop();
                matrix.case("openssl_decrypt", arguments, None);
            }
        }
    }
    matrix.check("openssl_gcm_nonces");
}

#[test]
fn nist_aes_gcm_and_cbc_vectors() {
    let mut matrix = Matrix::new();
    let (ciphertext, tag) = matrix
        .case(
            "openssl_encrypt",
            vec![
                Value::from(vec![0; 16]),
                Value::from("aes-128-gcm"),
                Value::from(vec![0; 16]),
                Value::Int(1),
                Value::from(vec![0; 12]),
                Value::Null,
            ],
            Some(5),
        )
        .unwrap();
    // NIST GCM test data also used by the upstream PHP and RustCrypto tests.
    assert_eq!(ciphertext, hex("0388dace60b6a392f328c2b971b2fe78"));
    assert_eq!(tag, hex("ab6e47d42cec13bdf53a67b21257bddf"));

    let (ciphertext, _) = matrix
        .case(
            "openssl_encrypt",
            vec![
                hex("6bc1bee22e409f96e93d7e117393172a"),
                Value::from("aes-256-cbc"),
                hex("603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4"),
                Value::Int(3),
                hex("000102030405060708090a0b0c0d0e0f"),
            ],
            None,
        )
        .unwrap();
    // NIST SP 800-38A, F.2.5 AES-256-CBC encryption.
    assert_eq!(ciphertext, hex("f58c4c04d6e5f1ba779eabfb5f7bfbd6"));
    matrix.check("openssl_nist_vectors");
}

fn hex(encoded: &str) -> Value {
    Value::from(
        (0..encoded.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>(),
    )
}
