//! Password hashes cross over: Phera verifies what PHP made and PHP verifies
//! what Phera made. Salts are random, so the hashes themselves never match.

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::support::{Comparison, invoke, php_call, registry};

/// An options array of integers, such as `['cost' => 4]`.
fn options(values: &[(&str, i64)]) -> Value {
    let mut options = PhpArray::new();
    for &(key, value) in values {
        options.insert(ArrayKey::str(key), Value::Int(value));
    }
    Value::from(options)
}

/// What one line of PHP prints.
fn php_output(name: &str, script: &str) -> Vec<String> {
    oracle::run_php(name, &format!("<?php {script}")).expect("PHP 8.5 required")
}

#[tokio::test]
async fn hashes_verify_in_both_runtimes() {
    let registry = registry();
    let mut context = TestContext::default();
    for (algorithm, options) in [
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
        let algorithm = Value::from(algorithm);
        for password in [
            b"".to_vec(),
            b"password".to_vec(),
            vec![0xff; 80],
            vec![b'x'; 72],
        ] {
            let password = Value::from(password);
            let arguments = [password.clone(), algorithm.clone(), options.clone()];
            let script = format!("echo {};", php_call("password_hash", &arguments));
            let php_hash = Value::from(php_output("password-create", &script).remove(0));

            let verify = vec![password.clone(), php_hash.clone()];
            let verified = invoke(&registry, &mut context, "password_verify", verify).await;
            assert_eq!(verified.unwrap(), Value::Bool(true));
            let verify = vec![Value::from("wrong"), php_hash];
            let verified = invoke(&registry, &mut context, "password_verify", verify).await;
            assert_eq!(verified.unwrap(), Value::Bool(false));

            let hash = invoke(&registry, &mut context, "password_hash", arguments.to_vec());
            let rust_hash = hash.await.unwrap();
            let verify = php_call("password_verify", &[password, rust_hash.clone()]);
            let script = format!("echo {verify} ? 'yes' : 'no';");
            assert_eq!(php_output("password-verify", &script), ["yes"]);

            let rehash = vec![rust_hash.clone(), algorithm.clone(), options.clone()];
            let needs_rehash = call(&registry, &mut context, "password_needs_rehash", rehash);
            assert_eq!(needs_rehash.unwrap(), Value::Bool(false));

            let mut comparison = Comparison::new();
            let info = vec![rust_hash];
            comparison
                .record_call(&registry, &mut context, "password_get_info", info)
                .await;
            comparison.compare("password-info", "password_get_info of a Phera hash");
        }
    }
}

#[tokio::test]
async fn crypt_schemes_and_malformed_salts() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
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
            let arguments = vec![Value::from(password), Value::from(salt)];
            let expression = php_call("crypt", &arguments);
            let result = invoke(&registry, &mut context, "crypt", arguments).await;
            comparison.record_expression(&expression, &result, &mut context);
        }
    }
    comparison.compare("crypt-matrix", "crypt");
}
