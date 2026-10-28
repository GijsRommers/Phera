//! Expected outputs were taken from PHP 8.5's `json_encode()`.

use phera_ext_json::Json;
use phera_extension_api::{CallError, Registry, Value};

fn encode(args: &[Value]) -> Result<Value, CallError> {
    let mut registry = Registry::new();
    registry.load(&Json);
    registry.get_function("json_encode").unwrap()(args)
}

fn encode_with(value: impl Into<Value>, flags: i64) -> Value {
    encode(&[value.into(), Value::Int(flags)]).unwrap()
}

fn json(value: impl Into<Value>) -> Value {
    encode_with(value, 0)
}

fn constant(name: &str) -> i64 {
    let mut registry = Registry::new();
    registry.load(&Json);
    match registry.get_constant(name) {
        Some(Value::Int(i)) => *i,
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn scalars() {
    assert_eq!(encode(&[Value::Null]), Ok(Value::from("null")));
    assert_eq!(json(true), Value::from("true"));
    assert_eq!(json(i64::MAX), Value::from("9223372036854775807"));
}

#[test]
fn floats() {
    let cases = [
        (1.0, "1", "1.0"),
        (0.1, "0.1", "0.1"),
        (1e16, "10000000000000000", "10000000000000000.0"),
        (1e17, "1.0e+17", "1.0e+17"),
        (1e25, "1.0e+25", "1.0e+25"),
        (0.0001, "0.0001", "0.0001"),
        (0.00001, "1.0e-5", "1.0e-5"),
        (-0.0, "-0", "-0.0"),
        (1.5e-7, "1.5e-7", "1.5e-7"),
        (
            123456789012345678.0,
            "1.2345678901234568e+17",
            "1.2345678901234568e+17",
        ),
    ];
    let preserve = constant("JSON_PRESERVE_ZERO_FRACTION");
    for (f, plain, preserved) in cases {
        assert_eq!(json(f), Value::from(plain), "{f}");
        assert_eq!(encode_with(f, preserve), Value::from(preserved), "{f}");
    }
}

#[test]
fn inf_and_nan() {
    assert_eq!(json(f64::NAN), Value::Bool(false));
    let partial = constant("JSON_PARTIAL_OUTPUT_ON_ERROR");
    assert_eq!(encode_with(f64::INFINITY, partial), Value::from("0"));
    assert_eq!(
        encode(&[
            Value::Float(f64::NAN),
            Value::Int(constant("JSON_THROW_ON_ERROR"))
        ]),
        Err(CallError::Error(
            "Inf and NaN cannot be JSON encoded".to_owned()
        ))
    );
}

#[test]
fn string_escaping() {
    assert_eq!(
        json("a/é\x01\"\\\u{1F600}"),
        Value::from("\"a\\/\\u00e9\\u0001\\\"\\\\\\ud83d\\ude00\"")
    );
    assert_eq!(
        encode_with(
            "a/é",
            constant("JSON_UNESCAPED_SLASHES") | constant("JSON_UNESCAPED_UNICODE")
        ),
        Value::from("\"a/é\"")
    );
    let hex = [
        "JSON_HEX_TAG",
        "JSON_HEX_AMP",
        "JSON_HEX_APOS",
        "JSON_HEX_QUOT",
    ]
    .map(constant)
    .into_iter()
    .fold(0, |acc, flag| acc | flag);
    assert_eq!(
        encode_with("<>&'\"", hex),
        Value::from("\"\\u003C\\u003E\\u0026\\u0027\\u0022\"")
    );
    assert_eq!(json("\x7f"), Value::from("\"\x7f\""));
}

#[test]
fn line_terminators() {
    let unicode = constant("JSON_UNESCAPED_UNICODE");
    assert_eq!(json("\u{2028}"), Value::from("\"\\u2028\""));
    assert_eq!(encode_with("\u{2028}", unicode), Value::from("\"\\u2028\""));
    assert_eq!(
        encode_with(
            "\u{2028}",
            unicode | constant("JSON_UNESCAPED_LINE_TERMINATORS")
        ),
        Value::from("\"\u{2028}\"")
    );
}

#[test]
fn invalid_utf8() {
    assert_eq!(json(b"\xff".to_vec()), Value::Bool(false));
    assert_eq!(
        encode_with(b"\xff".to_vec(), constant("JSON_INVALID_UTF8_SUBSTITUTE")),
        Value::from("\"\\ufffd\"")
    );
    assert_eq!(
        encode_with(b"\xffa".to_vec(), constant("JSON_INVALID_UTF8_IGNORE")),
        Value::from("\"a\"")
    );
    assert_eq!(
        encode_with(b"\xff".to_vec(), constant("JSON_PARTIAL_OUTPUT_ON_ERROR")),
        Value::from("null")
    );
}

#[test]
fn argument_errors() {
    assert_eq!(
        encode(&[]),
        Err(CallError::ArgumentCount {
            min: 1,
            max: 3,
            given: 0
        })
    );
    assert_eq!(
        encode(&[Value::Null, Value::from("x")]),
        Err(CallError::ArgumentType {
            position: 2,
            expected: "int",
            given: "string"
        })
    );
}
