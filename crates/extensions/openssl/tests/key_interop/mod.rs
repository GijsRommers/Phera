//! Exchange PKCS#8 encrypted keys and RSA messages in both directions.

use std::cell::RefCell;
use std::rc::Rc;

use phera_extension_api::Value;
use phera_extension_api::testing::{call, oracle};

use super::{Matrix, fixtures, native, unhex};

#[test]
fn exchange_rsa_messages_and_password_protected_keys() {
    use std::fmt::Write;
    let keys = fixtures();
    let (private, public) = &keys[0];
    let (registry, mut context) = native();
    let mut matrix = Matrix::new();
    let mut script = String::from("<?php\n");
    let public_pem = oracle::php_literal(public);
    let data = Value::from(b"asymmetric message\0\xff".to_vec());
    let plaintext = oracle::php_literal(&data);
    let padding_options = [
        (1, Value::Null),
        (4, Value::Null),
        (4, Value::from("sha256")),
    ];
    for (padding, algorithm) in &padding_options {
        let reference = Rc::new(RefCell::new(Value::Null));
        let call_result = call(
            &registry,
            &mut context,
            "openssl_public_encrypt",
            vec![
                data.clone(),
                Value::Reference(reference.clone()),
                public.clone(),
                Value::Int(*padding),
                algorithm.clone(),
            ],
        )
        .unwrap();
        assert_eq!(call_result, Value::Bool(true));
        matrix.case(
            "openssl_private_decrypt",
            vec![
                reference.take(),
                Value::Null,
                private.clone(),
                Value::Int(*padding),
                algorithm.clone(),
            ],
            Some(1),
        );
        let digest = oracle::php_literal(algorithm);
        writeln!(
            script,
            "if (!openssl_public_encrypt({plaintext}, $out, {public_pem}, {padding}, {digest})) \
             exit(1); echo bin2hex($out), \"\\n\";"
        )
        .unwrap();
    }
    for (private, _) in &keys {
        let reference = Rc::new(RefCell::new(Value::Null));
        let password = Value::from("encryption passphrase");
        let exported = call(
            &registry,
            &mut context,
            "openssl_pkey_export",
            vec![
                private.clone(),
                Value::Reference(reference.clone()),
                password.clone(),
            ],
        )
        .unwrap();
        assert_eq!(exported, Value::Bool(true));
        let encrypted = reference.take();
        matrix.case(
            "openssl_pkey_get_private",
            vec![encrypted.clone(), password.clone()],
            None,
        );
        matrix.case(
            "openssl_pkey_get_private",
            vec![encrypted, Value::from("wrong phrase")],
            None,
        );
        let private = oracle::php_literal(private);
        let pass = oracle::php_literal(&password);
        writeln!(
            script,
            "if (!openssl_pkey_export({private}, $out, {pass})) exit(1); \
             echo bin2hex($out), \"\\n\";"
        )
        .unwrap();
    }
    let rows = oracle::run_php("openssl_rsa_and_encrypted_keys", &script).unwrap();
    assert_eq!(rows.len(), 9);
    for ((padding, algorithm), ciphertext) in padding_options.into_iter().zip(&rows) {
        let reference = Rc::new(RefCell::new(Value::Null));
        let result = call(
            &registry,
            &mut context,
            "openssl_private_decrypt",
            vec![
                unhex(ciphertext),
                Value::Reference(reference.clone()),
                private.clone(),
                Value::Int(padding),
                algorithm,
            ],
        )
        .unwrap();
        assert_eq!(result, Value::Bool(true));
        assert_eq!(reference.take(), data);
    }
    for encoded in &rows[3..] {
        matrix.case(
            "openssl_pkey_get_private",
            vec![unhex(encoded), Value::from("encryption passphrase")],
            None,
        );
        matrix.case(
            "openssl_pkey_get_private",
            vec![unhex(encoded), Value::from("wrong phrase")],
            None,
        );
    }
    for (index, (private, public)) in keys.iter().enumerate() {
        if matches!(index, 1..=3 | 5) {
            matrix.case(
                "openssl_pkey_derive",
                vec![public.clone(), private.clone()],
                None,
            );
            for size in [0, 1, 16, 32, 256, -1] {
                matrix.case(
                    "openssl_pkey_derive",
                    vec![public.clone(), private.clone(), Value::Int(size)],
                    None,
                );
            }
        }
    }
    matrix.check("openssl_rsa_and_encrypted_keys");
}
