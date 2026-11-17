//! Password hashes must interoperate in both directions; random salts prevent direct equality.
use phera_ext_hash::Hash;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{
    ArrayKey, Extension, NativeFn, NativeResult, PhpArray, Registry, Value, prepare_arguments,
};
async fn invoke(
    registry: &Registry,
    context: &mut TestContext,
    name: &str,
    mut args: Vec<Value>,
) -> NativeResult {
    let entry = registry.get_function(name).unwrap();
    prepare_arguments(entry, &mut args, context)?;
    match entry.handler {
        NativeFn::Sync(f) => f(context, &args),
        NativeFn::Async(f) => f(context, args).await,
    }
}
fn options(values: &[(&str, i64)]) -> Value {
    let mut result = PhpArray::new();
    for (k, v) in values {
        result.insert(ArrayKey::str(*k), Value::Int(*v));
    }
    Value::from(result)
}
#[tokio::test]
async fn hashes_verify_in_both_runtimes() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    for (algorithm, opts) in [
        ("2y", options(&[("cost", 4)])),
        (
            "argon2i",
            options(&[("memory_cost", 32), ("time_cost", 2), ("threads", 2)]),
        ),
        (
            "argon2id",
            options(&[("memory_cost", 32), ("time_cost", 2), ("threads", 1)]),
        ),
    ] {
        for password in [
            b"".to_vec(),
            b"password".to_vec(),
            vec![0xff; 80],
            vec![b'x'; 72],
        ] {
            let password = Value::from(password);
            let php = oracle::run_php(
                "password-create",
                &format!(
                    "<?php echo password_hash({}, {}, {});",
                    oracle::php_literal(&password),
                    oracle::php_literal(&Value::from(algorithm)),
                    oracle::php_literal(&opts)
                ),
            )
            .expect("PHP required");
            let php_hash = Value::from(php[0].clone());
            assert_eq!(
                invoke(
                    &registry,
                    &mut context,
                    "password_verify",
                    vec![password.clone(), php_hash.clone()]
                )
                .await
                .unwrap(),
                Value::Bool(true)
            );
            assert_eq!(
                invoke(
                    &registry,
                    &mut context,
                    "password_verify",
                    vec![Value::from("wrong"), php_hash]
                )
                .await
                .unwrap(),
                Value::Bool(false)
            );
            let rust_hash = invoke(
                &registry,
                &mut context,
                "password_hash",
                vec![password.clone(), Value::from(algorithm), opts.clone()],
            )
            .await
            .unwrap();
            let php = oracle::run_php(
                "password-verify",
                &format!(
                    "<?php echo password_verify({}, {}) ? 'yes' : 'no';",
                    oracle::php_literal(&password),
                    oracle::php_literal(&rust_hash)
                ),
            )
            .expect("PHP required");
            assert_eq!(php, ["yes"]);
            assert_eq!(
                call(
                    &registry,
                    &mut context,
                    "password_needs_rehash",
                    vec![rust_hash.clone(), Value::from(algorithm), opts.clone()]
                )
                .unwrap(),
                Value::Bool(false)
            );
            let php = oracle::run_php(
                "password-info",
                &format!(
                    "<?php {} run('info',fn()=>password_get_info({}));",
                    oracle::PHP_PREAMBLE,
                    oracle::php_literal(&rust_hash)
                ),
            )
            .unwrap();
            let result = call(
                &registry,
                &mut context,
                "password_get_info",
                vec![rust_hash],
            );
            assert_eq!(
                php,
                [oracle::encode_case(
                    "info",
                    &result,
                    &context.output,
                    &context.diagnostics
                )]
            );
        }
    }
    println!("password interoperability: 72 comparisons, zero mismatches");
}
#[tokio::test]
async fn crypt_schemes_and_malformed_salts() {
    let mut registry = Registry::new();
    Hash.register(&mut registry);
    let mut context = TestContext::default();
    let mut script = format!("<?php {}", oracle::PHP_PREAMBLE);
    let mut rust = vec![];
    let mut labels = vec![];
    for salt in [
        "ab",
        "_J9..CCCC",
        "$1$salt$",
        "$5$salt$",
        "$6$salt$",
        "$5$rounds=1000$salt$",
        "$6$rounds=1000$salt$",
        "$2y$04$abcdefghijklmnopqrstuu",
        "",
        "*0",
        "*1",
        "a!",
        "$7$bad",
        "$2y$03$bad",
    ] {
        for password in [
            b"".to_vec(),
            b"password".to_vec(),
            b"a\0b".to_vec(),
            vec![0xff; 80],
        ] {
            let args = vec![Value::from(password), Value::from(salt)];
            let expression = format!(
                "crypt({},{})",
                oracle::php_literal(&args[0]),
                oracle::php_literal(&args[1])
            );
            script.push_str(&format!("run('crypt',fn()=>{expression});\n"));
            let result = invoke(&registry, &mut context, "crypt", args).await;
            rust.push(oracle::encode_case(
                "crypt",
                &result,
                &context.output,
                &context.diagnostics,
            ));
            labels.push(expression);
        }
    }
    let php = oracle::run_php("crypt-matrix", &script).expect("PHP required");
    oracle::compare("crypt", &labels, &php, &rust);
}
