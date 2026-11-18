//! Every Randomizer method on the three seedable engines, including
//! arguments PHP rejects, then engine jumps; and the secure functions,
//! whose output can only be checked for plausibility.

use phera_extension_api::testing::{TestContext, call, call_method};
use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::support::{Comparison, engine, php_arguments, randomizer, registry};

#[test]
fn randomizer_methods() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    for class in ["Mt19937", "PcgOneseq128XslRr64", "Xoshiro256StarStar"] {
        let engine = engine(&registry, &mut context, class, vec![Value::Int(42)]);
        let randomizer = randomizer(&registry, &mut context, &engine);
        comparison.setup(&format!(
            "$e = new Random\\Engine\\{class}(42); $r = new Random\\Randomizer($e);"
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
            let expression = format!("$r->{name}({})", php_arguments(&arguments));
            let result = call_method(&registry, &mut context, &randomizer, name, arguments);
            comparison.record(&expression, &result, &mut context);
        }
        for _ in 0..10 {
            if class != "Mt19937" {
                let arguments = if class.starts_with("Pcg") {
                    vec![Value::Int(123456789)]
                } else {
                    vec![]
                };
                comparison.setup(&format!("$e->jump({});", php_arguments(&arguments)));
                call_method(&registry, &mut context, &engine, "jump", arguments).unwrap();
            }
            let result = call_method(&registry, &mut context, &engine, "generate", vec![]);
            comparison.record("$e->generate()", &result, &mut context);
        }
    }
    comparison.compare("random-methods", "Randomizer methods");
}

#[test]
fn secure_sanity_and_errors() {
    let registry = registry();
    let mut context = TestContext::default();
    // 100,000 draws from 16 values: each should come up about 6,250 times.
    let mut bins = [0usize; 16];
    for _ in 0..100_000 {
        let arguments = vec![Value::Int(0), Value::Int(15)];
        let Ok(Value::Int(value)) = call(&registry, &mut context, "random_int", arguments) else {
            panic!("random_int() returns an int");
        };
        bins[value as usize] += 1;
    }
    for count in bins {
        assert!(
            (5500..7000).contains(&count),
            "secure distribution bin {count}"
        );
    }
    let mut random_bytes = || {
        call(
            &registry,
            &mut context,
            "random_bytes",
            vec![Value::Int(4096)],
        )
    };
    assert_ne!(random_bytes().unwrap(), random_bytes().unwrap());

    let mut comparison = Comparison::new();
    for (name, arguments) in [
        ("mt_rand", vec![Value::Int(1)]),
        ("rand", vec![Value::Int(1)]),
        ("mt_rand", vec![Value::Int(2), Value::Int(1)]),
        ("random_int", vec![Value::Int(2), Value::Int(1)]),
        ("random_bytes", vec![Value::Int(0)]),
        ("random_bytes", vec![Value::Int(-1)]),
    ] {
        let expression = format!("{name}({})", php_arguments(&arguments));
        let result = call(&registry, &mut context, name, arguments);
        comparison.record(&expression, &result, &mut context);
    }
    comparison.compare("random-errors", "random errors");
}
