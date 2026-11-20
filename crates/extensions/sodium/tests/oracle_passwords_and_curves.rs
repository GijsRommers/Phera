//! Compare PHP's password hashes, Phera's curve operations and their diagnostics.

mod support;

use phera_extension_api::{Value, testing::oracle};
use support::{Matrix, repeated};

#[tokio::test]
async fn password_and_curve_cases() {
    if oracle::find_php().is_none() {
        return;
    }
    let mut matrix = Matrix::new();
    let secret = repeated(7, 32);
    let mut entries = vec![
        ("crypto_box_publickey", vec![repeated(7, 64)]),
        ("crypto_box_secretkey", vec![repeated(7, 64)]),
        (
            "crypto_box",
            vec![Value::from("box"), repeated(4, 24), repeated(7, 64)],
        ),
        (
            "crypto_box_open",
            vec![Value::from("box"), repeated(4, 24), repeated(7, 64)],
        ),
        (
            "crypto_box_seal_open",
            vec![Value::from("box"), repeated(7, 64)],
        ),
        (
            "crypto_kx_client_session_keys",
            vec![repeated(7, 64), secret.clone()],
        ),
        (
            "crypto_kx_server_session_keys",
            vec![repeated(7, 64), secret.clone()],
        ),
        ("crypto_kx_publickey", vec![repeated(7, 64)]),
        ("crypto_kx_secretkey", vec![repeated(7, 64)]),
        ("crypto_sign_secretkey", vec![repeated(7, 96)]),
        ("crypto_sign_publickey", vec![repeated(7, 96)]),
        ("crypto_core_ristretto255_from_hash", vec![repeated(7, 64)]),
        (
            "crypto_core_ristretto255_is_valid_point",
            vec![repeated(0, 32)],
        ),
        (
            "crypto_core_ristretto255_add",
            vec![repeated(0, 32), repeated(0, 32)],
        ),
        (
            "crypto_core_ristretto255_sub",
            vec![repeated(0, 32), repeated(0, 32)],
        ),
        (
            "crypto_scalarmult_ristretto255",
            vec![secret.clone(), repeated(0, 32)],
        ),
        ("crypto_scalarmult_ristretto255_base", vec![secret.clone()]),
        (
            "crypto_core_ristretto255_scalar_reduce",
            vec![repeated(7, 64)],
        ),
    ];
    for name in ["add", "sub", "mul"] {
        let full_name = format!("sodium_crypto_core_ristretto255_scalar_{name}");
        matrix
            .vary(&full_name, vec![secret.clone(), secret.clone()])
            .await;
    }
    for name in ["complement", "invert", "negate"] {
        let full_name = format!("sodium_crypto_core_ristretto255_scalar_{name}");
        matrix.vary(&full_name, vec![secret.clone()]).await;
    }
    let password = Value::from(b"test\0\xff".as_slice());
    for algorithm in [1, 2, 3] {
        for size in [0, 15, 16, 32, 65, i64::MAX] {
            for limits in [(0, 8192), (1, 8192), (3, 8191), (3, 8192)] {
                let args = vec![
                    Value::Int(size),
                    password.clone(),
                    repeated(0, 16),
                    Value::Int(limits.0),
                    Value::Int(limits.1),
                    Value::Int(algorithm),
                ];
                matrix.case("sodium_crypto_pwhash", args).await;
            }
        }
    }
    entries.extend([
        (
            "crypto_pwhash_str_verify",
            vec![Value::from("malformed"), password.clone()],
        ),
        (
            "crypto_pwhash_str_needs_rehash",
            vec![Value::from("malformed"), Value::Int(2), Value::Int(8192)],
        ),
        (
            "crypto_pwhash_scryptsalsa208sha256_str_verify",
            vec![Value::from("malformed"), password.clone()],
        ),
    ]);
    for (name, args) in entries {
        matrix.vary(&format!("sodium_{name}"), args).await;
    }
    for (ops, mem) in [(0, 0), (3, 1), (524_287, 1), (524_288, 16_777_216)] {
        matrix
            .case(
                "sodium_crypto_pwhash_scryptsalsa208sha256",
                vec![
                    Value::Int(32),
                    password.clone(),
                    repeated(0, 32),
                    Value::Int(ops),
                    Value::Int(mem),
                ],
            )
            .await;
    }
    matrix.compare("sodium_passwords_and_curves");
}
