//! Procedural shuffles and key selection consume exactly the shared PHP MT sequence.
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{
    ArrayKey, Extension, NativeFn, PhpArray, Registry, Value, prepare_arguments,
};
use std::{cell::RefCell, rc::Rc};
fn registry() -> Registry {
    let mut r = Registry::new();
    phera_ext_core::Core.register(&mut r);
    phera_ext_random::Random.register(&mut r);
    r
}
#[test]
fn shuffle_and_keys() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    for mode in [0, 1] {
        for seed in [-1, 0, 42, i64::MAX] {
            call(
                &registry,
                &mut context,
                "mt_srand",
                vec![Value::Int(seed), Value::Int(mode)],
            )
            .unwrap();
            script.push_str(&format!("mt_srand({seed},{mode});\n"));
            for count in 0..=20 {
                let mut array = PhpArray::new();
                for i in 0..count {
                    array.push(Value::Int(i)).unwrap();
                }
                let literal = oracle::php_literal(&Value::from(array.clone()));
                let reference = Rc::new(RefCell::new(Value::from(array.clone())));
                context.diagnostics.entries.clear();
                call(
                    &registry,
                    &mut context,
                    "shuffle",
                    vec![Value::Reference(reference.clone())],
                )
                .unwrap();
                let result = Ok(reference.borrow().clone());
                rust.push(oracle::encode_case(
                    "shuffle",
                    &result,
                    &context.output,
                    &context.diagnostics,
                ));
                script.push_str(&format!(
                    "run('shuffle',function(){{$a={literal};shuffle($a);return $a;}});\n"
                ));
                for name in ["str_shuffle", "array_rand"] {
                    let arguments = if name == "str_shuffle" {
                        vec![Value::from("abcdef".repeat(count as usize))]
                    } else {
                        vec![Value::from(array.clone()), Value::Int(1)]
                    };
                    context.diagnostics.entries.clear();
                    let expression = format!(
                        "{name}({})",
                        arguments
                            .iter()
                            .map(oracle::php_literal)
                            .collect::<Vec<_>>()
                            .join(",")
                    );
                    script.push_str(&format!("run('case',fn()=>{expression});\n"));
                    let result = call(&registry, &mut context, name, arguments);
                    rust.push(oracle::encode_case(
                        "case",
                        &result,
                        &context.output,
                        &context.diagnostics,
                    ));
                }
                if count > 3 {
                    array.remove(&ArrayKey::Int(1));
                    let result = call(
                        &registry,
                        &mut context,
                        "array_rand",
                        vec![Value::from(array)],
                    );
                    script.push_str(&format!(
                        "run('case',function(){{$a={literal};\
                unset($a[1]);return array_rand($a);}});"
                    ));
                    rust.push(oracle::encode_case(
                        "case",
                        &result,
                        &context.output,
                        &context.diagnostics,
                    ));
                }
            }
        }
    }
    // The seed calls are setup; their deprecations are independently covered in methods.rs.
    script = script.replace("mt_srand(", "@mt_srand(");
    let php = oracle::run_php("random-procedural", &script).unwrap();
    oracle::compare("procedural collections", &[], &php, &rust);
}
#[tokio::test]
async fn unique_ids_and_lcg_properties() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut ids = std::collections::HashSet::new();
    for entropy in [false, true] {
        for _ in 0..1000 {
            let entry = registry.get_function("uniqid").unwrap();
            let mut args = vec![Value::from("prefix"), Value::Bool(entropy)];
            prepare_arguments(entry, &mut args, &mut context).unwrap();
            let NativeFn::Async(f) = entry.handler else {
                panic!("async")
            };
            let Value::String(value) = f(&mut context, args).await.unwrap() else {
                panic!("string")
            };
            assert!(value.as_bytes().starts_with(b"prefix"));
            assert_eq!(value.as_bytes().len(), if entropy { 29 } else { 19 });
            assert!(ids.insert(value.as_bytes().to_vec()));
        }
    }
    for _ in 0..1000 {
        let Value::Float(value) = call(&registry, &mut context, "lcg_value", vec![]).unwrap()
        else {
            panic!("float")
        };
        assert!(value > 0.0 && value < 1.0);
    }
    let diagnostics = &context.diagnostics.entries;
    assert_eq!(diagnostics.len(), 1000);
    let php = oracle::run_php(
        "random-lcg-deprecation",
        &format!(
            "<?php {} run('lcg',function(){{$x=lcg_value();return $x>0&&$x<1;}});",
            oracle::PHP_PREAMBLE
        ),
    )
    .unwrap();
    context.diagnostics.entries.truncate(1);
    assert_eq!(
        php,
        [oracle::encode_case(
            "lcg",
            &Ok(Value::Bool(true)),
            &[],
            &context.diagnostics
        )]
    );
}
