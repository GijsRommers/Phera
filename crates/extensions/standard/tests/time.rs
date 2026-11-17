//! The clock functions cannot be compared with PHP value by value, since
//! time moves. These tests check their shape and that they agree with each
//! other; `sleep` and `usleep` are tested through the server in the CLI crate.

use phera_ext_standard::Standard;
use phera_extension_api::testing::{TestContext, call};
use phera_extension_api::{Extension, Registry, Value};

fn registry() -> Registry {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    registry
}

fn invoke(name: &str, arguments: Vec<Value>) -> Value {
    let registry = registry();
    let mut context = TestContext::default();
    call(&registry, &mut context, name, arguments).expect("call succeeds")
}

#[test]
fn time_is_a_plausible_unix_timestamp() {
    let Value::Int(seconds) = invoke("time", vec![]) else {
        panic!("time() returns an int");
    };
    // After 1 January 2026 and before the year 2100.
    assert!((1_767_225_600..4_102_444_800).contains(&seconds));
}

#[test]
fn microtime_has_both_forms() {
    let Value::Float(seconds) = invoke("microtime", vec![Value::Bool(true)]) else {
        panic!("microtime(true) returns a float");
    };
    assert!(seconds > 1_767_225_600.0);

    let text = invoke("microtime", vec![]);
    let Value::String(text) = text else {
        panic!("microtime() returns a string");
    };
    let text = text.to_string_lossy();
    let (fraction, whole) = text.split_once(' ').expect("two parts");
    assert_eq!(fraction.len(), "0.12345600".len());
    assert!(fraction.starts_with("0."));
    assert!(whole.parse::<i64>().expect("seconds") > 1_767_225_600);
}

#[test]
fn hrtime_only_moves_forward() {
    let Value::Int(first) = invoke("hrtime", vec![Value::Bool(true)]) else {
        panic!("hrtime(true) returns an int");
    };
    let Value::Int(second) = invoke("hrtime", vec![Value::Bool(true)]) else {
        panic!("hrtime(true) returns an int");
    };
    assert!(second >= first);

    let Value::Array(parts) = invoke("hrtime", vec![]) else {
        panic!("hrtime() returns an array");
    };
    assert_eq!(parts.len(), 2);
}
