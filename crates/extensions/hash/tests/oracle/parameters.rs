//! KDFs, seeds and secrets, argument types and stored password hashes, with
//! PHP's exact errors and diagnostics.

use phera_extension_api::testing::TestContext;
use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::support::{Comparison, registry};

/// `[$key => $value]`
fn options(key: &str, value: Value) -> Value {
    let mut options = PhpArray::new();
    options.insert(ArrayKey::str(key), value);
    Value::from(options)
}

#[tokio::test]
async fn parameter_matrix() {
    let mut cases = Vec::new();
    let mut case = |name: &'static str, arguments: Vec<Value>| cases.push((name, arguments));
    for name in ["hash_algos", "hash_hmac_algos", "password_algos"] {
        case(name, vec![]);
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
                    case(
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
                case(
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
            let options = options("seed", seed);
            for length in [0, 1, 7, 16, 32, 64, 240, 241, 1024] {
                let data = Value::from("x".repeat(length));
                let arguments = vec![Value::from(algo), data, Value::Bool(false), options.clone()];
                case("hash", arguments);
            }
        }
    }
    for algo in ["xxh3", "xxh128"] {
        for length in [0, 1, 135, 136, 192, 256, 257, 512] {
            let options = options("secret", Value::from("s".repeat(length)));
            for data_length in [0, 1, 17, 240, 241, 1024] {
                // PHP hangs on a short secret with long data; xxhash_secrets.rs
                // covers these cases with a time limit.
                if length < 136 && data_length == 1024 {
                    continue;
                }
                let data = Value::from("x".repeat(data_length));
                let arguments = vec![Value::from(algo), data, Value::Bool(false), options.clone()];
                case("hash", arguments);
            }
        }
    }
    for value in [
        Value::Null,
        Value::Bool(false),
        Value::Int(-1),
        Value::Float(1.5),
        Value::from(vec![0xff, 0]),
        Value::from(PhpArray::new()),
    ] {
        case("hash", vec![value.clone(), Value::from("x")]);
        case("hash_equals", vec![value.clone(), Value::from("x")]);
        case("hash", vec![Value::from("sha256"), value.clone()]);
        case("md5", vec![value.clone()]);
        case("sha1", vec![value]);
    }
    for hash in [
        "",
        "$2y$04$invalid",
        "$2y$04$.....................................................",
        "$argon2i$",
        "$argon2id$v=19$m=8,t=1,p=1$bad",
        "$argon2id$v=19$m=8,t=no,p=2$bad",
    ] {
        case("password_get_info", vec![Value::from(hash)]);
        for algo in [
            Value::Null,
            Value::Int(0),
            Value::Int(1),
            Value::Int(2),
            Value::from("bad"),
            Value::from("2y"),
            Value::from("argon2id"),
        ] {
            case("password_needs_rehash", vec![Value::from(hash), algo]);
        }
    }
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    for (name, arguments) in cases {
        comparison
            .record_call(&registry, &mut context, name, arguments)
            .await;
    }
    comparison.compare("hash-parameters", "hash parameters");
}
