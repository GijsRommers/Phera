//! Every algorithm `hash_algos()` lists, on every input length from 0 to 300
//! and around the block sizes up to 1 MiB, in binary and in hexadecimal.

use phera_extension_api::Value;
use phera_extension_api::testing::{TestContext, call, oracle};

use crate::support::{Comparison, registry};

#[test]
fn every_algorithm_and_length() {
    let registry = registry();
    let mut context = TestContext::default();
    let Ok(Value::Array(names)) = call(&registry, &mut context, "hash_algos", vec![]) else {
        panic!("hash_algos() returns an array");
    };
    let mut comparison = Comparison::new();
    for name in names.iter().map(|(_, name)| name) {
        for length in (0usize..=300).chain([511, 512, 513, 1023, 1024, 1025, 1_048_576]) {
            let data: Vec<u8> = (0..length).map(|i| (i * 31 + 17) as u8).collect();
            for binary in [false, true] {
                let result = call(
                    &registry,
                    &mut context,
                    "hash",
                    vec![name.clone(), Value::from(data.clone()), Value::Bool(binary)],
                );
                // The same bytes, built in PHP instead of written out.
                let expression = format!(
                    "hash({}, substr(str_repeat(implode('', array_map(\
                     fn($i) => chr(($i * 31 + 17) & 255), range(0, 255))), {}), 0, {length}), {binary})",
                    oracle::php_literal(name),
                    length.div_ceil(256).max(1),
                );
                comparison.record_expression(&expression, &result, &mut context);
            }
        }
    }
    comparison.compare("hash-algorithms", "all hash algorithms");
}
