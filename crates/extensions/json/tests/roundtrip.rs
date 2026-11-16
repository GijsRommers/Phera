//! Generated values exercise encode/decode composition without losing PHP types.
use phera_ext_json::Json;
use phera_extension_api::testing::{TestContext, call};
use phera_extension_api::{ArrayKey, Extension, PhpArray, Registry, Value};
#[test]
fn generated_roundtrips() {
    let mut registry = Registry::new();
    Json.register(&mut registry);
    let mut context = TestContext::default();
    for index in 0..2000 {
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("number"), Value::Int(index));
        properties.insert(
            ArrayKey::str("string"),
            Value::from(format!("é😀/{index}\n\0")),
        );
        properties.insert(ArrayKey::str("fraction"), Value::Float(index as f64 / 8.0));
        let mut nested = PhpArray::new();
        nested.push(Value::Bool(index % 2 == 0)).unwrap();
        nested.push(Value::Null).unwrap();
        properties.insert(ArrayKey::str("nested"), Value::from(nested));
        let original = Value::from(properties);
        let encoded = call(
            &registry,
            &mut context,
            "json_encode",
            vec![original.clone(), Value::Int(1024)],
        )
        .unwrap();
        let decoded = call(
            &registry,
            &mut context,
            "json_decode",
            vec![encoded, Value::Bool(true)],
        )
        .unwrap();
        assert_eq!(original, decoded);
    }
}
#[test]
fn requests_and_throwing_calls_preserve_error_contract() {
    let mut registry = Registry::new();
    Json.register(&mut registry);
    let mut first = TestContext::default();
    let mut second = TestContext::default();
    call(&registry, &mut first, "json_decode", vec![Value::from("[")]).unwrap();
    assert_eq!(
        call(&registry, &mut first, "json_last_error", vec![]).unwrap(),
        Value::Int(4)
    );
    assert_eq!(
        call(&registry, &mut second, "json_last_error", vec![]).unwrap(),
        Value::Int(0)
    );
    call(
        &registry,
        &mut first,
        "json_encode",
        vec![Value::Null, Value::Int(4194304)],
    )
    .unwrap();
    assert!(
        call(
            &registry,
            &mut first,
            "json_decode",
            vec![
                Value::from("\"\\ud800\""),
                Value::Bool(true),
                Value::Int(512),
                Value::Int(4194304)
            ]
        )
        .is_err()
    );
    assert_eq!(
        call(&registry, &mut first, "json_last_error", vec![]).unwrap(),
        Value::Int(4)
    );
}
