//! A million draws per engine and range shape, compared through a SHA-256
//! of all of them plus the first and last hundred; and the procedural
//! aliases, which share one generator per request.

use phera_extension_api::Value;
use phera_extension_api::testing::{TestContext, call, call_method, oracle};
use sha2::{Digest, Sha256};

use crate::support::{Comparison, engine, php_arguments, randomizer, registry};

const DRAWS: usize = 1_000_000;

/// How many draws at each end are compared one by one.
const EDGE: usize = 100;

#[test]
fn million_draws_per_engine_and_range() {
    let registry = registry();
    let mut context = TestContext::default();
    let ranges = [
        None,
        Some((1, 6)),
        Some((0, 255)),
        Some((-100, 100)),
        Some((0, 2147483648)),
        Some((0, u32::MAX as i64)),
        Some((0, 4294967296)),
        Some((i64::MIN, i64::MAX)),
        Some((i64::MIN, 0)),
        Some((7, 7)),
    ];
    // Mt19937's legacy mode is deprecated; that is compared in methods.rs.
    let mut script = "<?php\nerror_reporting(0);\n".to_string();
    let mut rust = vec![];
    let mut labels = vec![];
    for (class, mode) in [
        ("Mt19937", None),
        ("PcgOneseq128XslRr64", None),
        ("Xoshiro256StarStar", None),
        ("Mt19937", Some(1)),
    ] {
        let constructor: Vec<Value> = [Some(42), mode]
            .into_iter()
            .flatten()
            .map(Value::Int)
            .collect();
        for range in ranges {
            let engine = engine(&registry, &mut context, class, constructor.clone());
            let randomizer = randomizer(&registry, &mut context, &engine);
            let (method, arguments) = match range {
                None => ("nextInt", vec![]),
                Some((min, max)) => ("getInt", vec![Value::Int(min), Value::Int(max)]),
            };
            script.push_str(&format!(
                "$r = new Random\\Randomizer(new Random\\Engine\\{class}({}));\n\
                 $h = hash_init('sha256'); $edges = [];\n\
                 for ($i = 0; $i < {DRAWS}; $i++) {{ $v = $r->{method}({}); hash_update($h, pack('q', $v)); \
                 if ($i < {EDGE} || $i >= {last}) $edges[] = $v; }}\n\
                 echo hash_final($h), '|', implode(',', $edges), \"\\n\";\n",
                php_arguments(&constructor),
                php_arguments(&arguments),
                last = DRAWS - EDGE,
            ));
            let mut checksum = Sha256::new();
            let mut edges = vec![];
            for index in 0..DRAWS {
                let draw = call_method(
                    &registry,
                    &mut context,
                    &randomizer,
                    method,
                    arguments.clone(),
                );
                let Ok(Value::Int(value)) = draw else {
                    panic!("{method}() returns an int");
                };
                checksum.update(value.to_le_bytes());
                if !(EDGE..DRAWS - EDGE).contains(&index) {
                    edges.push(value.to_string());
                }
            }
            let checksum: String = checksum
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            rust.push(format!("{checksum}|{}", edges.join(",")));
            labels.push(format!(
                "{class}{} {range:?}",
                if mode.is_some() { " legacy" } else { "" }
            ));
        }
    }
    let php = oracle::run_php("random-million-sequences", &script).expect("PHP 8.5 required");
    oracle::compare("40 million random draws", &labels, &php, &rust);
}

#[test]
fn procedural_aliases_share_request_state() {
    let registry = registry();
    let mut context = TestContext::default();
    // A second request, seeded the same way, must draw the same numbers
    // although the first one alternates between the two functions.
    let mut other = TestContext::default();
    call(&registry, &mut context, "mt_srand", vec![Value::Int(42)]).unwrap();
    call(&registry, &mut other, "srand", vec![Value::Int(42)]).unwrap();
    let mut comparison = Comparison::new();
    comparison.setup("mt_srand(42);");
    for index in 0..10000 {
        let name = if index % 2 == 0 { "rand" } else { "mt_rand" };
        let range = vec![Value::Int(-5), Value::Int(7)];
        let result = call(&registry, &mut context, name, range.clone());
        let isolated = call(&registry, &mut other, "mt_rand", range.clone()).unwrap();
        assert_eq!(result.as_ref().unwrap(), &isolated);
        let expression = format!("{name}({})", php_arguments(&range));
        comparison.record(&expression, &result, &mut context);
    }
    comparison.compare("random-procedural-aliases", "request isolation and aliases");
}
