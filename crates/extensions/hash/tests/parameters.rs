//! KDFs, option coercions and malformed arguments retain PHP's exact diagnostics.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, Extension, PhpArray, Registry, Value};
struct Matrix {
    registry: Registry,
    context: TestContext,
    script: String,
    rust: Vec<String>,
    labels: Vec<String>,
}
impl Matrix {
    fn new() -> Self {
        let mut registry = Registry::new();
        Hash.register(&mut registry);
        Self {
            registry,
            context: TestContext::default(),
            script: format!("<?php {}", oracle::PHP_PREAMBLE),
            rust: vec![],
            labels: vec![],
        }
    }
    fn case(&mut self, name: &str, arguments: Vec<Value>) {
        self.context.diagnostics.entries.clear();
        self.context.output.clear();
        let expression = format!(
            "{name}({})",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        self.script
            .push_str(&format!("run('case',fn()=>{expression});\n"));
        let result = call(&self.registry, &mut self.context, name, arguments);
        self.rust.push(oracle::encode_case(
            "case",
            &result,
            &self.context.output,
            &self.context.diagnostics,
        ));
        self.labels.push(expression);
    }
    fn compare(self) {
        let php = oracle::run_php("hash-parameters", &self.script).expect("PHP required");
        oracle::compare("hash parameters", &self.labels, &php, &self.rust);
    }
}
#[test]
fn parameter_matrix() {
    let mut m = Matrix::new();
    for f in ["hash_algos", "hash_hmac_algos", "password_algos"] {
        m.case(f, vec![]);
    }
    for algo in [
        "sha256",
        "md5",
        "sha3-512",
        "haval128,3",
        "snefru",
        "gost",
        "xxh3",
        "bad",
        "",
    ] {
        for iterations in [-1, 0, 1, 2, 7] {
            for length in [-1, 0, 1, 2, 31, 32, 33, 100] {
                for binary in [false, true] {
                    m.case(
                        "hash_pbkdf2",
                        vec![
                            Value::from(algo),
                            Value::from("p\0w"),
                            Value::from("s\0t"),
                            Value::Int(iterations),
                            Value::Int(length),
                            Value::Bool(binary),
                        ],
                    );
                }
            }
        }
        for key in ["", "key", "k\0ey"] {
            for length in [-1, 0, 1, 32, 33, 510, 16321] {
                m.case(
                    "hash_hkdf",
                    vec![
                        Value::from(algo),
                        Value::from(key),
                        Value::Int(length),
                        Value::from("info"),
                        Value::from("salt"),
                    ],
                );
            }
        }
    }
    for algo in [
        "sha256", "murmur3a", "murmur3c", "murmur3f", "xxh32", "xxh64", "xxh3", "xxh128",
    ] {
        for seed in [
            Value::Null,
            Value::Bool(true),
            Value::from("42"),
            Value::Int(-1),
            Value::Int(42),
            Value::Int(i64::MAX),
            Value::Float(1.5),
        ] {
            let mut options = PhpArray::new();
            options.insert(ArrayKey::str("seed"), seed);
            for length in [0, 1, 7, 16, 32, 64, 240, 241, 1024] {
                m.case(
                    "hash",
                    vec![
                        Value::from(algo),
                        Value::from("x".repeat(length)),
                        Value::Bool(false),
                        Value::from(options.clone()),
                    ],
                );
            }
        }
    }
    for algo in ["xxh3", "xxh128"] {
        for length in [0, 1, 135, 136, 192, 256, 257, 512] {
            let mut options = PhpArray::new();
            options.insert(ArrayKey::str("secret"), Value::from("s".repeat(length)));
            for data_length in [0, 1, 17, 240, 241, 1024] {
                // These exact calls remain in oracle_timeout.rs, where a broken oracle
                // fails in two seconds instead of hanging the entire CI job.
                if length < 136 && data_length == 1024 {
                    continue;
                }

                m.case(
                    "hash",
                    vec![
                        Value::from(algo),
                        Value::from("x".repeat(data_length)),
                        Value::Bool(false),
                        Value::from(options.clone()),
                    ],
                );
            }
        }
    }
    let values = [
        Value::Null,
        Value::Bool(false),
        Value::Int(-1),
        Value::Float(1.5),
        Value::from(vec![0xff, 0]),
        Value::from(PhpArray::new()),
    ];
    for value in values {
        m.case("hash", vec![value.clone(), Value::from("x")]);
        m.case("hash_equals", vec![value.clone(), Value::from("x")]);
        m.case("hash", vec![Value::from("sha256"), value.clone()]);
        m.case("md5", vec![value.clone()]);
        m.case("sha1", vec![value]);
    }
    for hash in [
        "",
        "$2y$04$invalid",
        "$2y$04$.....................................................",
        "$argon2i$",
        "$argon2id$v=19$m=8,t=1,p=1$bad",
        "$argon2id$v=19$m=8,t=no,p=2$bad",
    ] {
        m.case("password_get_info", vec![Value::from(hash)]);
        for algo in [
            Value::Null,
            Value::Int(0),
            Value::Int(1),
            Value::Int(2),
            Value::from("bad"),
            Value::from("2y"),
            Value::from("argon2id"),
        ] {
            m.case("password_needs_rehash", vec![Value::from(hash), algo]);
        }
    }
    m.compare();
}
