//! Every constant registered by Standard is compared with the PHP 8.5 binary.

use phera_ext_standard::Standard;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};

#[test]
fn every_registered_constant_matches_php() {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    let mut entries: Vec<_> = registry.constants().collect();
    entries.sort_by_key(|(name, _)| *name);
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for (name, value) in entries {
        labels.push(name.to_string());
        script.push_str(&format!("run('{name}', fn() => constant('{name}'));\n"));
        rust.push(oracle::encode_case(
            name,
            &Ok(value.to_value()),
            &[],
            &phera_extension_api::value::CollectDiagnostics::default(),
        ));
    }
    let php = oracle::run_php("standard_constants", &script).expect("PHP oracle required");
    oracle::compare("standard constants", &labels, &php, &rust);
    let mut context = TestContext::default();
    let pad_type = registry
        .get_constant("STR_PAD_LEFT")
        .expect("padding constant")
        .to_value();
    assert_eq!(
        call(
            &registry,
            &mut context,
            "str_pad",
            vec![Value::from("x"), Value::Int(5), Value::from(" "), pad_type]
        )
        .expect("pad"),
        Value::from("    x")
    );
}
