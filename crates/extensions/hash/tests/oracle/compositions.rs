//! HMAC with keys on both sides of every block size, and incremental hashing
//! split at every point of a 200-byte input, with `hash_copy()` in between.

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Registry, Value};

use crate::support::{Comparison, registry};

/// The names a listing function such as `hash_algos()` returns.
fn names(registry: &Registry, context: &mut TestContext, function: &str) -> Vec<Value> {
    let Ok(Value::Array(names)) = call(registry, context, function, vec![]) else {
        panic!("{function}() returns an array");
    };
    names.iter().map(|(_, name)| name.clone()).collect()
}

#[test]
fn hmac_matrix() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    for name in names(&registry, &mut context, "hash_hmac_algos") {
        for length in 0..=300 {
            for key_length in [0, 16, 32, 64, 72, 104, 128, 136, 144, 200] {
                for binary in [false, true] {
                    let result = call(
                        &registry,
                        &mut context,
                        "hash_hmac",
                        vec![
                            name.clone(),
                            Value::from("x".repeat(length)),
                            Value::from("k".repeat(key_length)),
                            Value::Bool(binary),
                        ],
                    );
                    let expression = format!(
                        "hash_hmac({}, str_repeat('x', {length}), str_repeat('k', {key_length}), {binary})",
                        oracle::php_literal(&name)
                    );
                    comparison.record_expression(&expression, &result, &mut context);
                }
            }
        }
    }
    comparison.compare(
        "hash-hmac-matrix",
        "HMAC all algorithms and key block boundaries",
    );
}

#[test]
fn incremental_every_split() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    for name in names(&registry, &mut context, "hash_algos") {
        for split in 0..=200 {
            for binary in [false, true] {
                let first = Value::from("x".repeat(split));
                let rest = Value::from("x".repeat(200 - split));
                let original = call(&registry, &mut context, "hash_init", vec![name.clone()]);
                let original = original.unwrap();
                let update = vec![original.clone(), first.clone()];
                call(&registry, &mut context, "hash_update", update).unwrap();
                let copy = call(&registry, &mut context, "hash_copy", vec![original.clone()]);
                let copy = copy.unwrap();
                call(
                    &registry,
                    &mut context,
                    "hash_update",
                    vec![copy.clone(), rest],
                )
                .unwrap();
                let finish = vec![copy, Value::Bool(binary)];
                let result = call(&registry, &mut context, "hash_final", finish);
                let closure = format!(
                    "function () {{ $h = hash_init({name}); hash_update($h, str_repeat('x', {split})); \
                     $c = hash_copy($h); hash_update($c, str_repeat('x', {rest})); \
                     return hash_final($c, {binary}); }}",
                    name = oracle::php_literal(&name),
                    rest = 200 - split,
                );
                comparison.record(&closure, &result, &mut context);

                // Updating the copy must have left the original alone.
                let finish = vec![original, Value::Bool(binary)];
                let original = call(&registry, &mut context, "hash_final", finish).unwrap();
                let whole = vec![name.clone(), first, Value::Bool(binary)];
                let expected = call(&registry, &mut context, "hash", whole).unwrap();
                assert_eq!(original, expected, "copy must not mutate original");
            }
        }
    }
    comparison.compare("hash-incremental", "incremental every split and copy");
}
