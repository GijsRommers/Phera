//! Password option validation and malformed hashes are differential tests, not guessed errors.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{
    ArrayKey, Extension, NativeFn, NativeResult, PhpArray, Registry, Value, prepare_arguments,
};
async fn invoke(
    r: &Registry,
    c: &mut TestContext,
    name: &str,
    mut args: Vec<Value>,
) -> NativeResult {
    let e = r.get_function(name).unwrap();
    prepare_arguments(e, &mut args, c)?;
    match e.handler {
        NativeFn::Sync(f) => f(c, &args),
        NativeFn::Async(f) => f(c, args).await,
    }
}
#[tokio::test]
async fn options_and_malformed_hashes() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let mut cases = vec![];
    for (algo, option, values) in [
        ("2y", "cost", vec![-1, 0, 3, 4, 5, 12, 32]),
        ("argon2i", "memory_cost", vec![-1, 0, 7, 8, 32, 4294967296]),
        ("argon2id", "time_cost", vec![-1, 0, 1, 2, 4294967296]),
        ("argon2id", "threads", vec![-1, 0, 1, 2, 16777216]),
    ] {
        for value in values {
            let mut opts = PhpArray::new();
            if algo != "2y" {
                opts.insert(ArrayKey::str("memory_cost"), Value::Int(32));
                opts.insert(ArrayKey::str("time_cost"), Value::Int(1));
            }
            opts.insert(ArrayKey::str(option), Value::Int(value));
            cases.push(vec![
                Value::from("password"),
                Value::from(algo),
                Value::from(opts),
            ]);
        }
    }
    for value in [
        Value::Null,
        Value::Bool(true),
        Value::from("4"),
        Value::Float(4.5),
        Value::from(PhpArray::new()),
    ] {
        let mut opts = PhpArray::new();
        opts.insert(ArrayKey::str("cost"), value);
        cases.push(vec![
            Value::from("password"),
            Value::from("2y"),
            Value::from(opts),
        ]);
    }
    for algorithm in [
        Value::Null,
        Value::Int(-1),
        Value::Int(0),
        Value::Int(1),
        Value::from("bad"),
        Value::from("2y"),
    ] {
        let mut opts = PhpArray::new();
        opts.insert(ArrayKey::str("cost"), Value::Int(4));
        opts.insert(ArrayKey::str("salt"), Value::from("ignored"));
        cases.push(vec![Value::from("password"), algorithm, Value::from(opts)]);
    }
    cases.push(vec![Value::from("nul\0byte"), Value::from("2y")]);
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    let mut labels = vec![];
    for arguments in cases {
        context.diagnostics.entries.clear();
        let expression = format!(
            "password_get_info(password_hash({}))",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        script.push_str(&format!("run('password',fn()=>{expression});\n"));
        let result = match invoke(&registry, &mut context, "password_hash", arguments).await {
            Ok(hash) => call(&registry, &mut context, "password_get_info", vec![hash]),
            Err(error) => Err(error),
        };
        rust.push(oracle::encode_case(
            "password",
            &result,
            &context.output,
            &context.diagnostics,
        ));
        labels.push(expression);
    }
    for hash in [
        "",
        "*0",
        "*1",
        "not a hash",
        "$2y$04$bad",
        "$argon2i$",
        "$argon2id$v=19$m=8,t=1,p=1$bad$bad",
        "$argon2id$v=19$m=0,t=0,p=0$bad$bad",
        "ab12345678901",
    ] {
        context.diagnostics.entries.clear();
        let expression = format!(
            "password_verify('password',{})",
            oracle::php_literal(&Value::from(hash))
        );
        script.push_str(&format!("run('password',fn()=>{expression});"));
        let result = invoke(
            &registry,
            &mut context,
            "password_verify",
            vec![Value::from("password"), Value::from(hash)],
        )
        .await;
        rust.push(oracle::encode_case(
            "password",
            &result,
            &context.output,
            &context.diagnostics,
        ));
        labels.push(expression);
    }
    let php = oracle::run_php("password-validation", &script).unwrap();
    oracle::compare("password validation", &labels, &php, &rust);
}
