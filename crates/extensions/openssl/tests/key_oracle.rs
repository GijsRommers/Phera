//! Compare PHP key details and exchange signatures, encrypted keys, and RSA messages.

use std::cell::RefCell;
use std::rc::Rc;

use phera_ext_openssl::OpenSsl;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, Registry, Value};

mod key_interop;
mod support;
use support::Matrix;

type KeyPair = (Value, Value);

fn fixtures(name: &str) -> Vec<KeyPair> {
    let php = r#"<?php
    $options = [['private_key_bits' => 2048]];
    foreach (['prime256v1', 'secp384r1', 'secp521r1'] as $curve) {
        $options[] = ['private_key_type' => OPENSSL_KEYTYPE_EC, 'curve_name' => $curve];
    }
    $options[] = ['private_key_type' => OPENSSL_KEYTYPE_ED25519];
    $options[] = ['private_key_type' => OPENSSL_KEYTYPE_X25519];
    foreach ($options as $option) {
        $key = openssl_pkey_new($option);
        if (!openssl_pkey_export($key, $private)) exit(1);
        $details = openssl_pkey_get_details($key);
        echo bin2hex($private), ' ', bin2hex($details['key']), "\n";
    }
    "#;
    let rows = oracle::run_php(name, php).expect("PHP oracle required");
    assert_eq!(rows.len(), 6);
    rows.iter()
        .map(|row| {
            let (private, public) = row.split_once(' ').expect("two keys");
            (unhex(private), unhex(public))
        })
        .collect()
}

fn unhex(hex: &str) -> Value {
    assert!(hex.len().is_multiple_of(2));
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("oracle returned hex"))
        .collect::<Vec<_>>();
    Value::from(bytes)
}

fn native() -> (Registry, TestContext) {
    let mut registry = Registry::new();
    registry.load(&OpenSsl);
    (registry, TestContext::default())
}

#[test]
fn match_key_and_signature_behaviour() {
    let keys = fixtures("openssl_key_matrix_fixtures");
    let mut matrix = Matrix::new();
    for (private, public) in &keys {
        for value in [private.clone(), public.clone(), Value::Null, Value::Int(1)] {
            matrix.case("openssl_pkey_get_private", vec![value.clone()], None);
            matrix.case("openssl_pkey_get_public", vec![value], None);
        }
    }
    for value in [
        Value::from("bad"),
        Value::from(b"\0\xff".to_vec()),
        Value::from(""),
        Value::from("-----"),
    ] {
        matrix.case("openssl_pkey_get_private", vec![value.clone()], None);
        matrix.case("openssl_pkey_get_public", vec![value.clone()], None);
        for data in [
            Value::from("x"),
            Value::from(""),
            Value::Int(123),
            Value::Null,
        ] {
            for algorithm in [
                Value::Int(0),
                Value::Int(7),
                Value::Int(600),
                Value::from("unknown"),
            ] {
                matrix.case(
                    "openssl_sign",
                    vec![
                        data.clone(),
                        Value::from("sentinel"),
                        value.clone(),
                        algorithm.clone(),
                    ],
                    Some(1),
                );
                matrix.case(
                    "openssl_verify",
                    vec![
                        data.clone(),
                        Value::from("signature"),
                        value.clone(),
                        algorithm,
                    ],
                    None,
                );
            }
        }
    }
    let mut empty_ecdsa = Matrix::new();
    for (index, (_, public)) in keys.iter().enumerate() {
        let algorithm = if index == 4 { 0 } else { 7 };
        for signature in [Value::from(""), Value::from("x"), Value::from(vec![0; 256])] {
            let empty_ec_signature = matches!(index, 1..=3)
                && matches!(&signature, Value::String(bytes) if bytes.is_empty());
            let cases = if empty_ec_signature {
                &mut empty_ecdsa
            } else {
                &mut matrix
            };
            let result = cases.case(
                "openssl_verify",
                vec![
                    Value::from("message"),
                    signature,
                    public.clone(),
                    Value::Int(algorithm),
                ],
                None,
            );
            if empty_ec_signature {
                assert_eq!(result, Some((Value::Int(-1), Value::Null)));
            }
        }
    }
    if oracle::supports_php_since("empty ECDSA signatures on P-256, P-384 and P-521", 80_513) {
        empty_ecdsa.check("openssl_empty_ecdsa_signatures");
    }
    for (index, (private, public)) in keys.iter().enumerate() {
        let (registry, mut context) = native();
        let algorithms = [
            Value::Int(0),
            Value::Int(1),
            Value::Int(2),
            Value::Int(6),
            Value::Int(7),
            Value::Int(8),
            Value::Int(9),
            Value::Int(10),
            Value::from("SHA256"),
            Value::from("sha3-256"),
            Value::from("sha512-224"),
        ];
        for length in [0, 1, 7, 31, 128, 255] {
            let data = Value::from((0..length).map(|index| index as u8).collect::<Vec<_>>());
            for algorithm in &algorithms {
                for padding in [0, 1, 6, 99] {
                    let mut arguments = vec![
                        data.clone(),
                        Value::from("sentinel"),
                        private.clone(),
                        algorithm.clone(),
                        Value::Int(padding),
                    ];
                    let deterministic_or_failed = match index {
                        0 => padding != 6,
                        1..=3 => padding != 0,
                        _ => true,
                    };
                    let (signed, signature) = if deterministic_or_failed {
                        matrix.case("openssl_sign", arguments, Some(1)).unwrap()
                    } else {
                        let reference = Rc::new(RefCell::new(arguments[1].clone()));
                        arguments[1] = Value::Reference(reference.clone());
                        let signed =
                            call(&registry, &mut context, "openssl_sign", arguments).unwrap();
                        (signed, reference.take())
                    };
                    if signed == Value::Bool(true) {
                        for message in [data.clone(), Value::from("altered message")] {
                            matrix.case(
                                "openssl_verify",
                                vec![
                                    message,
                                    signature.clone(),
                                    public.clone(),
                                    algorithm.clone(),
                                    Value::Int(padding),
                                ],
                                None,
                            );
                        }
                    }
                }
            }
        }
    }
    matrix.check("openssl_key_and_signature");
}

#[test]
fn php_signatures_and_key_details_are_read_by_phera() {
    use std::fmt::Write;
    let keys = fixtures("openssl_signature_fixtures");
    let (registry, mut context) = native();
    let mut script = String::from("<?php\n");
    let data = Value::from(b"fixed \0\xff message".to_vec());
    for (index, (private, _)) in keys.iter().enumerate() {
        let key = oracle::php_literal(private);
        writeln!(script, "$key = openssl_pkey_get_private({key});").unwrap();
        writeln!(
            script,
            "echo bin2hex(serialize(openssl_pkey_get_details($key))), \"\\n\";"
        )
        .unwrap();
        let algorithm = if index == 4 { 0 } else { 7 };
        if index < 5 {
            let padding = if index == 0 { 6 } else { 0 };
            let message = oracle::php_literal(&data);
            writeln!(
                script,
                "if (!openssl_sign({message}, $out, $key, {algorithm}, {padding})) exit(1);"
            )
            .unwrap();
            writeln!(script, "echo bin2hex($out), \"\\n\";").unwrap();
        }
    }
    let rows = oracle::run_php("openssl_php_signatures", &script).expect("PHP oracle required");
    assert_eq!(rows.len(), 11);
    let mut rows = rows.iter();
    for (index, (private, public)) in keys.iter().enumerate() {
        let key = call(
            &registry,
            &mut context,
            "openssl_pkey_get_private",
            vec![private.clone()],
        )
        .unwrap();
        let detail = call(
            &registry,
            &mut context,
            "openssl_pkey_get_details",
            vec![key],
        )
        .unwrap();
        let Value::Array(detail) = detail else {
            panic!("key details should be an array")
        };
        assert_eq!(detail.get(&ArrayKey::str("key")), Some(public));
        let expected = unhex(rows.next().unwrap());
        let (oracle_script, label) = (
            oracle::php_literal(&expected),
            oracle::php_literal(&Value::from(detail.as_ref().clone())),
        );
        let script = format!("<?php\nvar_dump(unserialize({oracle_script}) === {label});");
        assert_eq!(
            oracle::run_php(&format!("openssl_details_{index}"), &script).unwrap(),
            ["bool(true)"],
        );
        if index < 5 {
            let algorithm = if index == 4 { 0 } else { 7 };
            let padding = if index == 0 { 6 } else { 0 };
            let verified = call(
                &registry,
                &mut context,
                "openssl_verify",
                vec![
                    data.clone(),
                    unhex(rows.next().unwrap()),
                    public.clone(),
                    Value::Int(algorithm),
                    Value::Int(padding),
                ],
            )
            .unwrap();
            assert_eq!(verified, Value::Int(1), "key scheme {index}");
        }
    }
}
