//! Randomizer collections, float sampling, argument diagnostics and secure sanity.
use phera_extension_api::testing::{TestContext, call, call_method, instantiate, oracle};
use phera_extension_api::{ArrayKey, Extension, PhpArray, Registry, Value};
fn registry() -> Registry {
    let mut r = Registry::new();
    phera_ext_core::Core.register(&mut r);
    phera_ext_random::Random.register(&mut r);
    r
}
#[test]
fn randomizer_methods() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    let mut labels = vec![];
    for class in ["Mt19937", "PcgOneseq128XslRr64", "Xoshiro256StarStar"] {
        let engine = instantiate(&registry, &format!("Random\\Engine\\{class}")).unwrap();
        call_method(
            &registry,
            &mut context,
            &engine,
            "__construct",
            vec![Value::Int(42)],
        )
        .unwrap();
        let randomizer = instantiate(&registry, "Random\\Randomizer").unwrap();
        call_method(
            &registry,
            &mut context,
            &randomizer,
            "__construct",
            vec![Value::Object(engine.clone())],
        )
        .unwrap();
        script.push_str(&format!(
            "$e=new Random\\Engine\\{class}(42);$r=new Random\\Randomizer($e);\n"
        ));
        let mut cases = vec![];
        for length in [-1, 0, 1, 2, 7, 8, 9, 16, 32, 1000] {
            cases.push(("getBytes", vec![Value::Int(length)]));
            for source in [
                "".to_string(),
                "a".into(),
                "abc".into(),
                "abcdefgh".into(),
                "a".repeat(257),
            ] {
                cases.push((
                    "getBytesFromString",
                    vec![Value::from(source), Value::Int(length)],
                ));
            }
        }
        for count in 0..=20 {
            let mut array = PhpArray::new();
            for i in 0..count {
                array.insert(ArrayKey::str(format!("k{i}")), Value::Int(i));
            }
            for requested in [-1, 0, 1, 2, 4, 10, 20] {
                cases.push((
                    "pickArrayKeys",
                    vec![Value::from(array.clone()), Value::Int(requested)],
                ));
            }
            cases.push(("shuffleArray", vec![Value::from(array)]));
            cases.push((
                "shuffleBytes",
                vec![Value::from("0123456789".repeat(count as usize))],
            ));
        }
        for _ in 0..1000 {
            cases.push(("nextFloat", vec![]));
        }
        for (min, max) in [
            (0.0, 1.0),
            (-1.0, 1.0),
            (-100.0, 200.0),
            (1e100, 1e200),
            (-1e300, 1e300),
            (f64::MIN_POSITIVE, 1.0),
            (0.0, f64::from_bits(1)),
            (f64::INFINITY, 1.0),
            (1.0, 0.0),
        ] {
            for _ in 0..50 {
                cases.push(("getFloat", vec![Value::Float(min), Value::Float(max)]));
            }
        }
        for (name, arguments) in cases {
            context.diagnostics.entries.clear();
            let expression = format!(
                "$r->{name}({})",
                arguments
                    .iter()
                    .map(oracle::php_literal)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            script.push_str(&format!("run('method',fn()=>{expression});\n"));
            let result = call_method(&registry, &mut context, &randomizer, name, arguments);
            rust.push(oracle::encode_case(
                "method",
                &result,
                &context.output,
                &context.diagnostics,
            ));
            labels.push(expression);
        }
        for _ in 0..10 {
            if class != "Mt19937" {
                let args = if class.starts_with("Pcg") {
                    vec![Value::Int(123456789)]
                } else {
                    vec![]
                };
                call_method(&registry, &mut context, &engine, "jump", args.clone()).unwrap();
                script.push_str(&format!(
                    "$e->jump({});",
                    args.iter()
                        .map(oracle::php_literal)
                        .collect::<Vec<_>>()
                        .join(",")
                ));
            }
            let result = call_method(&registry, &mut context, &engine, "generate", vec![]);
            script.push_str("run('engine',fn()=>$e->generate());\n");
            rust.push(oracle::encode_case(
                "engine",
                &result,
                &context.output,
                &context.diagnostics,
            ));
            labels.push(class.into());
        }
    }
    let php = oracle::run_php("random-methods", &script).unwrap();
    oracle::compare("Randomizer methods", &labels, &php, &rust);
}
#[test]
fn secure_sanity_and_errors() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut bins = [0usize; 16];
    for _ in 0..100_000 {
        let Value::Int(value) = call(
            &registry,
            &mut context,
            "random_int",
            vec![Value::Int(0), Value::Int(15)],
        )
        .unwrap() else {
            panic!("int")
        };
        bins[value as usize] += 1;
    }
    for count in bins {
        assert!(
            (5500..7000).contains(&count),
            "secure distribution bin {count}"
        );
    }
    let a = call(
        &registry,
        &mut context,
        "random_bytes",
        vec![Value::Int(4096)],
    )
    .unwrap();
    let b = call(
        &registry,
        &mut context,
        "random_bytes",
        vec![Value::Int(4096)],
    )
    .unwrap();
    assert_ne!(a, b);
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    for (name, args) in [
        ("mt_rand", vec![Value::Int(1)]),
        ("rand", vec![Value::Int(1)]),
        ("mt_rand", vec![Value::Int(2), Value::Int(1)]),
        ("random_int", vec![Value::Int(2), Value::Int(1)]),
        ("random_bytes", vec![Value::Int(0)]),
        ("random_bytes", vec![Value::Int(-1)]),
        ("mt_srand", vec![Value::Int(42), Value::Int(1)]),
    ] {
        context.diagnostics.entries.clear();
        script.push_str(&format!(
            "run('error',fn()=>{name}({}));",
            args.iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        ));
        let result = call(&registry, &mut context, name, args);
        rust.push(oracle::encode_case(
            "error",
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }
    let php = oracle::run_php("random-errors", &script).unwrap();
    oracle::compare("random errors", &[], &php, &rust);
}
