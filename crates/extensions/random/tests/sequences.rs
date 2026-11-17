//! Million-draw sequences compare SHA-256 checksums plus the first and last 100 values.
use phera_extension_api::testing::{TestContext, call, call_method, instantiate, oracle};
use phera_extension_api::{Extension, Registry, Value};
use sha2::{Digest, Sha256};
fn registry() -> Registry {
    let mut r = Registry::new();
    phera_ext_core::Core.register(&mut r);
    phera_ext_random::Random.register(&mut r);
    r
}

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
    let mut script = "<?php\n".to_string();
    let mut rust = vec![];
    let mut labels = vec![];
    for engine in [
        "Mt19937",
        "PcgOneseq128XslRr64",
        "Xoshiro256StarStar",
        "legacy",
    ] {
        for range in ranges {
            let class = if engine == "legacy" {
                "Mt19937"
            } else {
                engine
            };
            let object = instantiate(&registry, &format!("Random\\Engine\\{class}")).unwrap();
            let mut arguments = vec![Value::Int(42)];
            if engine == "legacy" {
                arguments.push(Value::Int(1));
            }
            call_method(&registry, &mut context, &object, "__construct", arguments).unwrap();
            let randomizer = instantiate(&registry, "Random\\Randomizer").unwrap();
            call_method(
                &registry,
                &mut context,
                &randomizer,
                "__construct",
                vec![Value::Object(object)],
            )
            .unwrap();
            let method = if range.is_none() { "nextInt" } else { "getInt" };
            let args = range.map_or_else(Vec::new, |(min, max)| {
                vec![Value::Int(min), Value::Int(max)]
            });
            let expression = format!(
                "$r->{method}({})",
                args.iter()
                    .map(oracle::php_literal)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            script.push_str(&format!("$r=new Random\\Randomizer(new \
            Random\\Engine\\{class}(42{}));$h=hash_init('sha256');$edges=[];for($i=0;$i<1000000;$i++){{$v={expression};hash_update($h,pack('q',$v));if($i<100||$i>=999900)$edges[]=$v;}}echo hash_final($h),'|',implode(',',$edges),\"\\n\";\n",if engine=="legacy" {",1"} else {""}));
            let mut checksum = Sha256::new();
            let mut edges = vec![];
            for index in 0..1_000_000 {
                let Value::Int(value) =
                    call_method(&registry, &mut context, &randomizer, method, args.clone())
                        .unwrap()
                else {
                    panic!("int")
                };
                checksum.update(value.to_le_bytes());
                if !(100..999900).contains(&index) {
                    edges.push(value.to_string());
                }
            }
            rust.push(format!(
                "{}|{}",
                hex::encode(checksum.finalize()),
                edges.join(",")
            ));
            labels.push(format!("{engine} {range:?}"));
        }
    }
    // Numeric mode avoids the deprecated constant; capture the constructor deprecation separately.
    script = script.replace("<?php\n", "<?php\nerror_reporting(0);\n");
    let php = oracle::run_php("random-million-sequences", &script).expect("PHP 8.5 required");
    oracle::compare("40 million random draws", &labels, &php, &rust);
}
#[test]
fn procedural_aliases_share_request_state() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut other = TestContext::default();
    call(&registry, &mut context, "mt_srand", vec![Value::Int(42)]).unwrap();
    call(&registry, &mut other, "srand", vec![Value::Int(42)]).unwrap();
    let mut script = format!("<?php {} mt_srand(42);", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    for index in 0..10000 {
        let name = if index % 2 == 0 { "rand" } else { "mt_rand" };
        let result = call(
            &registry,
            &mut context,
            name,
            vec![Value::Int(-5), Value::Int(7)],
        );
        let isolated = call(
            &registry,
            &mut other,
            "mt_rand",
            vec![Value::Int(-5), Value::Int(7)],
        )
        .unwrap();
        assert_eq!(result.as_ref().unwrap(), &isolated);
        rust.push(oracle::encode_case(
            "draw",
            &result,
            &context.output,
            &context.diagnostics,
        ));
        script.push_str(&format!("run('draw',fn()=>{name}(-5,7));\n"));
    }
    let php = oracle::run_php("random-procedural-aliases", &script).unwrap();
    oracle::compare("request isolation and aliases", &[], &php, &rust);
}
