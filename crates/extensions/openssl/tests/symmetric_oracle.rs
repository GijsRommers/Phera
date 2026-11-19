//! PHP 8.5 differential coverage for cipher output, warnings, and output references.

use std::cell::RefCell;
use std::rc::Rc;

use phera_ext_openssl::OpenSsl;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{PhpArray, Registry, Value};

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

struct Matrix {
    script: String,
    lines: Vec<String>,
    labels: Vec<String>,
    registry: Registry,
    context: TestContext,
}

impl Matrix {
    fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&OpenSsl);
        Self {
            script: format!("<?php\n{}", oracle::PHP_PREAMBLE),
            lines: Vec::new(),
            labels: Vec::new(),
            registry,
            context: TestContext::default(),
        }
    }

    fn case(
        &mut self,
        function: &str,
        mut args: Vec<Value>,
        output: Option<usize>,
    ) -> Option<(Value, Value)> {
        use std::fmt::Write;
        let context = &mut self.context;
        context.output.clear();
        context.diagnostics.entries.clear();
        let label = self.lines.len().to_string();
        let mut literals: Vec<_> = args.iter().map(oracle::php_literal).collect();
        let reference = output.map(|index| {
            let value = Rc::new(RefCell::new(args[index].clone()));
            args[index] = Value::Reference(value.clone());
            write!(
                self.script,
                "run('{label}', function() {{ $ref = {}; ",
                literals[index]
            )
            .unwrap();
            literals[index] = "$ref".into();
            value
        });
        let expression = format!("{function}({})", literals.join(", "));
        if reference.is_some() {
            writeln!(
                self.script,
                "try {{ return {expression}; }} finally {{ echo enc($ref); }} }});"
            )
            .unwrap();
        } else {
            writeln!(self.script, "run('{label}', fn() => {expression});").unwrap();
        }
        let result = call(&self.registry, context, function, args);
        let value = reference.map_or(Value::Null, |reference| {
            let value = reference.borrow().clone();
            context
                .output
                .extend_from_slice(oracle::encode(&value).as_bytes());
            value
        });
        self.lines.push(oracle::encode_case(
            &label,
            &result,
            &context.output,
            &context.diagnostics,
        ));
        self.labels.push(expression);
        result.ok().map(|result| (result, value))
    }

    fn check(self, name: &str) {
        let expected = oracle::run_php(name, &self.script).expect("PHP oracle required");
        println!("{name}: {} compared calls", self.lines.len());
        oracle::compare(name, &self.labels, &expected, &self.lines);
    }
}
