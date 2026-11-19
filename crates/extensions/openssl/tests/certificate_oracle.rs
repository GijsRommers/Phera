//! Compare certificate input, signature verification, and fingerprints with PHP.

use phera_extension_api::testing::oracle;
use phera_extension_api::{PhpArray, Value};

mod support;
use support::Matrix;

#[test]
fn certificate_operations_match_php() {
    let php = r#"<?php
    $args = [
        ['private_key_bits' => 2048, 'digest_alg' => 'sha256'],
        ['private_key_type' => OPENSSL_KEYTYPE_EC, 'curve_name' => 'prime256v1',
            'digest_alg' => 'sha256'],
        ['private_key_type' => OPENSSL_KEYTYPE_ED25519, 'digest_alg' => 'null'],
    ];
    foreach ($args as $options) {
        $key = openssl_pkey_new($options);
        $csr = openssl_csr_new(['commonName' => 'Phera oracle'], $key, $options);
        $cert = openssl_csr_sign($csr, null, $key, 1, $options);
        if (!openssl_x509_export($cert, $pem) || !openssl_pkey_export($key, $private)) exit(1);
        echo bin2hex($pem), ' ', bin2hex($private), "\n";
    }
    "#;
    let php = oracle::run_php("openssl_certificate_fixtures", php).expect("PHP oracle required");
    assert_eq!(php.len(), 3);
    let mut matrix = Matrix::new();
    for row in &php {
        let (cert, private) = row.split_once(' ').unwrap();
        let cert = hex(cert);
        let private = hex(private);
        matrix.case("openssl_x509_read", vec![cert.clone()], None);
        matrix.case(
            "openssl_x509_export",
            vec![cert.clone(), Value::Null],
            Some(1),
        );
        matrix.case(
            "openssl_x509_verify",
            vec![cert.clone(), cert.clone()],
            None,
        );
        matrix.case(
            "openssl_x509_check_private_key",
            vec![cert.clone(), private],
            None,
        );
        matrix.case("openssl_pkey_get_public", vec![cert.clone()], None);
        for algorithm in ["sha1", "sha-256", "sha512", "sha3-384", "md5", "unknown"] {
            for raw in [false, true] {
                matrix.case(
                    "openssl_x509_fingerprint",
                    vec![cert.clone(), Value::from(algorithm), Value::Bool(raw)],
                    None,
                );
            }
        }
    }
    for invalid in [
        Value::from("not a certificate"),
        Value::from(PhpArray::new()),
        Value::Int(5),
    ] {
        matrix.case("openssl_x509_read", vec![invalid.clone()], None);
        matrix.case(
            "openssl_x509_export",
            vec![invalid.clone(), Value::Null],
            Some(1),
        );
        matrix.case("openssl_x509_fingerprint", vec![invalid.clone()], None);
        matrix.case(
            "openssl_x509_check_private_key",
            vec![invalid.clone(), Value::Null],
            None,
        );
        matrix.case("openssl_x509_verify", vec![invalid, Value::Null], None);
    }
    matrix.check("openssl_certificates");
}

fn hex(encoded: &str) -> Value {
    Value::from(
        (0..encoded.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&encoded[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>(),
    )
}
