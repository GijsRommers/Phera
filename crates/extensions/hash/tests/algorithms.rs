//! Every advertised algorithm is compared against PHP, in binary and hexadecimal.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};

#[test]
fn every_algorithm_and_length() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let algorithms = call(&registry, &mut context, "hash_algos", vec![]).unwrap();
    let Value::Array(names) = algorithms else {
        panic!("list")
    };
    let mut script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    let mut labels = vec![];
    for (_, name) in names.iter() {
        for length in (0usize..=300).chain([511, 512, 513, 1023, 1024, 1025, 1_048_576]) {
            let data: Vec<u8> = (0..length).map(|i| (i * 31 + 17) as u8).collect();
            for binary in [false, true] {
                let name_text = oracle::php_literal(name);
                script.push_str(&format!("run('hash',fn()=>hash({name_text}, \
            substr(str_repeat(implode('',array_map(fn($i)=>chr(($i*31+17)&255),range(0,255))),{}),0,{length}),{}));\n",length.div_ceil(256).max(1),if binary {"true"} else {"false"}));
                let result = call(
                    &registry,
                    &mut context,
                    "hash",
                    vec![name.clone(), Value::from(data.clone()), Value::Bool(binary)],
                );
                rust.push(oracle::encode_case(
                    "hash",
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
                labels.push(format!("{name_text} length={length} binary={binary}"));
            }
        }
    }
    let php = oracle::run_php("hash-algorithms", &script).expect("PHP 8.5 required");
    oracle::compare("all hash algorithms", &labels, &php, &rust);
}
