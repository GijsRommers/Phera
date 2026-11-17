//! HMAC and incremental state are compared with the same PHP calls, including empty chunks.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};

fn names(registry: &Registry, context: &mut TestContext, function: &str) -> Vec<Value> {
    let Value::Array(names) = call(registry, context, function, vec![]).unwrap() else {
        panic!("array")
    };
    names.iter().map(|(_, v)| v.clone()).collect()
}
#[test]
fn hmac_matrix() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let mut rust = vec![];
    let mut labels = vec![];
    let mut script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
    for name in names(&registry, &mut context, "hash_hmac_algos") {
        for length in 0..=300 {
            for key_length in [0, 16, 32, 64, 72, 104, 128, 136, 144, 200] {
                for binary in [false, true] {
                    let expression = format!(
                        "hash_hmac({},str_repeat('x',{length}),str_repeat('k',{key_length}),{binary})",
                        oracle::php_literal(&name)
                    );
                    script.push_str(&format!("run('hmac',fn()=>{expression});\n"));
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
                    rust.push(oracle::encode_case(
                        "hmac",
                        &result,
                        &context.output,
                        &context.diagnostics,
                    ));
                    labels.push(expression);
                }
            }
        }
    }
    let php = oracle::run_php("hash-hmac-matrix", &script).expect("PHP required");
    oracle::compare(
        "HMAC all algorithms and key block boundaries",
        &labels,
        &php,
        &rust,
    );
}
#[test]
fn incremental_every_split() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let mut rust = vec![];
    let mut labels = vec![];
    let mut script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
    for name in names(&registry, &mut context, "hash_algos") {
        for split in 0..=200 {
            for binary in [false, true] {
                let object =
                    call(&registry, &mut context, "hash_init", vec![name.clone()]).unwrap();
                call(
                    &registry,
                    &mut context,
                    "hash_update",
                    vec![object.clone(), Value::from("x".repeat(split))],
                )
                .unwrap();
                let copy =
                    call(&registry, &mut context, "hash_copy", vec![object.clone()]).unwrap();
                call(
                    &registry,
                    &mut context,
                    "hash_update",
                    vec![copy.clone(), Value::from("x".repeat(200 - split))],
                )
                .unwrap();
                let result = call(
                    &registry,
                    &mut context,
                    "hash_final",
                    vec![copy, Value::Bool(binary)],
                );
                script.push_str(&format!("run('split',function(){{$h=hash_init({});hash_update($h,str_repeat('x',{split}));$c=hash_copy($h);hash_update($c,str_repeat('x',{}));return hash_final($c,{binary});}});\n",oracle::php_literal(&name),200-split));
                rust.push(oracle::encode_case(
                    "split",
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
                labels.push(format!("{} split={split}", oracle::php_literal(&name)));
                let original = call(
                    &registry,
                    &mut context,
                    "hash_final",
                    vec![object, Value::Bool(binary)],
                )
                .unwrap();
                let expected = call(
                    &registry,
                    &mut context,
                    "hash",
                    vec![
                        name.clone(),
                        Value::from("x".repeat(split)),
                        Value::Bool(binary),
                    ],
                )
                .unwrap();
                assert_eq!(original, expected, "copy must not mutate original");
            }
        }
    }
    let php = oracle::run_php("hash-incremental", &script).expect("PHP required");
    oracle::compare("incremental every split and copy", &labels, &php, &rust);
}
