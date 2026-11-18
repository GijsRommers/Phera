//! Values that are not a `HashContext`, passed where one is expected, and a
//! `$stream` that is not a resource. PHP names `true` and `false` in these
//! errors, not `bool`.

use phera_extension_api::testing::TestContext;
use phera_extension_api::{PhpArray, Value};

use crate::support::{Comparison, invoke, php_call, registry};

#[tokio::test]
async fn values_that_are_not_contexts() {
    let registry = registry();
    let mut context = TestContext::default();
    let mut comparison = Comparison::new();
    let values = [
        Value::Bool(true),
        Value::Bool(false),
        Value::Null,
        Value::Int(1),
        Value::Float(1.5),
        Value::from("x"),
        Value::from(PhpArray::new()),
    ];
    for value in &values {
        for (name, rest) in [
            ("hash_update", vec![Value::from("x")]),
            ("hash_final", vec![]),
            ("hash_copy", vec![]),
            ("hash_update_stream", vec![Value::Null]),
            ("hash_update_file", vec![Value::from("x")]),
        ] {
            let arguments: Vec<Value> = [value.clone()].into_iter().chain(rest).collect();
            let expression = php_call(name, &arguments);
            let result = invoke(&registry, &mut context, name, arguments).await;
            comparison.record_expression(&expression, &result, &mut context);
        }
    }
    for stream in values {
        let closure = format!(
            "fn() => hash_update_stream(hash_init('md5'), {})",
            phera_extension_api::testing::oracle::php_literal(&stream)
        );
        let hash = invoke(
            &registry,
            &mut context,
            "hash_init",
            vec![Value::from("md5")],
        );
        let arguments = vec![hash.await.unwrap(), stream];
        let result = invoke(&registry, &mut context, "hash_update_stream", arguments).await;
        comparison.record(&closure, &result, &mut context);
    }
    comparison.compare("hash-context-errors", "values that are not a HashContext");
}
