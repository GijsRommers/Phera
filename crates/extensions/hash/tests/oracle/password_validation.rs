//! Password options at and beyond their limits, options of the wrong type,
//! ignored salts, and malformed stored hashes.

use phera_extension_api::testing::{TestContext, call};
use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::support::{Comparison, invoke, php_call, registry};

#[tokio::test]
async fn options_and_malformed_hashes() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut cases = vec![];
    for (algo, option, values) in [
        ("2y", "cost", vec![-1, 0, 3, 4, 5, 12, 32]),
        ("argon2i", "memory_cost", vec![-1, 0, 7, 8, 32, 4294967296]),
        ("argon2id", "time_cost", vec![-1, 0, 1, 2, 4294967296]),
        ("argon2id", "threads", vec![-1, 0, 1, 2, 16777216]),
    ] {
        for value in values {
            let mut options = PhpArray::new();
            if algo != "2y" {
                // Small Argon2 costs keep the test fast.
                options.insert(ArrayKey::str("memory_cost"), Value::Int(32));
                options.insert(ArrayKey::str("time_cost"), Value::Int(1));
            }
            options.insert(ArrayKey::str(option), Value::Int(value));
            cases.push(vec![
                Value::from("password"),
                Value::from(algo),
                Value::from(options),
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
        let mut options = PhpArray::new();
        options.insert(ArrayKey::str("cost"), value);
        cases.push(vec![
            Value::from("password"),
            Value::from("2y"),
            Value::from(options),
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
        let mut options = PhpArray::new();
        options.insert(ArrayKey::str("cost"), Value::Int(4));
        options.insert(ArrayKey::str("salt"), Value::from("ignored"));
        cases.push(vec![
            Value::from("password"),
            algorithm,
            Value::from(options),
        ]);
    }
    cases.push(vec![Value::from("nul\0byte"), Value::from("2y")]);

    let mut comparison = Comparison::new();
    for arguments in cases {
        // The hash is random, so compare what password_get_info() reads from it.
        let expression = format!(
            "password_get_info({})",
            php_call("password_hash", &arguments)
        );
        let result = match invoke(&registry, &mut context, "password_hash", arguments).await {
            Ok(hash) => call(&registry, &mut context, "password_get_info", vec![hash]),
            Err(error) => Err(error),
        };
        comparison.record_expression(&expression, &result, &mut context);
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
        let arguments = vec![Value::from("password"), Value::from(hash)];
        let expression = php_call("password_verify", &arguments);
        let result = invoke(&registry, &mut context, "password_verify", arguments).await;
        comparison.record_expression(&expression, &result, &mut context);
    }
    comparison.compare("password-validation", "password validation");
}
