//! Argument conversion checked against real PHP.
//!
//! Each probe is a real PHP function whose result shows what its argument was
//! converted to. The same call is made in PHP and through
//! `prepare_arguments` with a matching signature, over a matrix of values,
//! with and without `strict_types`.

use std::rc::Rc;

use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

struct StdClass;

impl phera_extension_api::value::ClassHandle for StdClass {
    fn name(&self) -> &str {
        "stdClass"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn list(items: Vec<Value>) -> Value {
    let mut array = PhpArray::new();
    for item in items {
        array.push(item).expect("push");
    }
    Value::Array(Rc::new(array))
}

/// PHP source and the equal Rust value.
fn values() -> Vec<(String, Value)> {
    let mut values: Vec<(String, Value)> = Vec::new();
    let mut add = |value: Value| values.push((oracle::php_literal(&value), value));

    add(Value::Null);
    add(Value::Bool(true));
    add(Value::Bool(false));
    for int in [0, 1, -1, 42, 255, i64::MAX, i64::MIN] {
        add(Value::Int(int));
    }
    for float in [
        0.0,
        -0.0,
        1.0,
        1.5,
        -1.5,
        0.1,
        -3.0,
        1e25,
        -1e25,
        9.223372036854775e18,
        9.223372036854776e18,
        -9.223372036854776e18,
        -9.223372036854778e18,
        1e-7,
        123456789012345680.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        add(Value::Float(float));
    }
    for string in [
        "",
        "0",
        "1",
        "abc",
        "12",
        " 12",
        "12 ",
        " 12 ",
        "12abc",
        "1.5",
        "1.0",
        "-1.5",
        "1e2",
        "1e400",
        "-0",
        "0x1A",
        "9223372036854775807",
        "9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
        ".5",
        "5.",
        "1e",
        "+3",
        "\t7\n",
        "NAN",
        "  ",
        "0.0",
        "true",
        "1 2",
        "\0",
    ] {
        add(Value::from(string));
    }
    add(list(vec![]));
    // Strings only: the str_replace() probe turns other elements into strings.
    add(list(vec![Value::from("1"), Value::from("2")]));
    let mut map = PhpArray::new();
    map.insert(ArrayKey::str("a"), Value::from("1"));
    map.insert(ArrayKey::int(7), Value::from("x"));
    add(Value::Array(Rc::new(map)));

    let object = Value::Object(Rc::new(phera_extension_api::value::Object::new(
        1,
        Rc::new(StdClass),
    )));
    values.push(("new stdClass".to_string(), object));
    values
}

/// The row `array_column` looks into: one entry for every key a converted
/// argument could become.
fn column_row() -> PhpArray {
    let mut row = PhpArray::new();
    let mut diagnostics = phera_extension_api::value::NoDiagnostics;
    for (_, value) in values() {
        let candidates = match &value {
            Value::Int(_) | Value::String(_) => vec![value.clone()],
            Value::Float(float) => {
                let text = phera_extension_api::value::to_php_string(&value, &mut diagnostics)
                    .expect("float to string");
                let mut candidates = vec![Value::String(text)];
                if float.is_finite() && float.abs() < 9e18 {
                    candidates.push(Value::Int(*float as i64));
                }
                candidates
            }
            _ => vec![],
        };
        for candidate in candidates {
            let key = ArrayKey::from_value(&candidate).expect("key");
            let label = format!("at {}", oracle::encode(&candidate));
            row.insert(key, Value::from(label));
        }
    }
    row
}

fn first(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(arguments[0].clone())
}

fn second(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(arguments[1].clone())
}

fn abs(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(match &arguments[0] {
        Value::Int(int) => int
            .checked_abs()
            .map_or(Value::Float(-(*int as f64)), Value::Int),
        Value::Float(float) => Value::Float(float.abs()),
        other => other.clone(),
    })
}

fn column(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let row = args::array(arguments, 0).expect("row");
    Ok(match &arguments[1] {
        Value::Null => list(vec![Value::Array(row.clone())]),
        key => {
            let key = ArrayKey::from_value(key).expect("key");
            list(row.get(&key).cloned().into_iter().collect())
        }
    })
}

fn third(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(arguments[2].clone())
}

fn is_object(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::object(arguments, 0).is_some()))
}

fn count_second(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(
        args::array(arguments, 1).map_or(-1, |array| array.len() as i64),
    ))
}

/// A probe: the PHP expression over `$v`, and the equal call in Rust.
struct Probe {
    tag: &'static str,
    php: &'static str,
    function: &'static str,
    arguments: fn(&Value) -> Vec<Value>,
}

fn registry() -> Registry {
    use ParamType::*;
    const STR_REPEAT: &[Param] = &[Param::new("string", String), Param::new("times", Int)];
    const INTDIV: &[Param] = &[Param::new("num1", Int), Param::new("num2", Int)];
    const FDIV: &[Param] = &[Param::new("num1", Float), Param::new("num2", Float)];
    const ARRAY_REVERSE: &[Param] = &[
        Param::new("array", Array),
        Param::new("preserve_keys", Bool).optional(),
    ];
    const ARRAY_SLICE: &[Param] = &[
        Param::new("array", Array),
        Param::new("offset", Int),
        Param::new("length", Int).nullable().optional(),
        Param::new("preserve_keys", Bool).optional(),
    ];
    const ABS: &[Param] = &[Param::new("num", Number)];
    const ARRAY_COLUMN: &[Param] = &[
        Param::new("array", Array),
        Param::new("column_key", IntOrString).nullable(),
        Param::new("index_key", IntOrString).nullable().optional(),
    ];
    const STR_REPLACE: &[Param] = &[
        Param::new("search", ArrayOrString),
        Param::new("replace", ArrayOrString),
        Param::new("subject", ArrayOrString),
        Param::new("count", Mixed).by_ref().optional(),
    ];
    const SPL_OBJECT_ID: &[Param] = &[Param::new("object", Object)];
    const ARRAY_MERGE: &[Param] = &[Param::new("arrays", Array).variadic()];

    let mut registry = Registry::new();
    for (name, params, handler) in [
        (
            "str_repeat",
            STR_REPEAT,
            first as phera_extension_api::SyncFn,
        ),
        ("intdiv", INTDIV, first),
        ("fdiv", FDIV, first),
        ("array_reverse", ARRAY_REVERSE, second),
        ("array_slice", ARRAY_SLICE, first),
        ("abs", ABS, abs),
        ("array_column", ARRAY_COLUMN, column),
        ("str_replace", STR_REPLACE, third),
        ("spl_object_id", SPL_OBJECT_ID, is_object),
        ("array_merge", ARRAY_MERGE, count_second),
    ] {
        registry.function(FunctionEntry::sync(name, params, handler));
    }
    registry
}

fn probes() -> Vec<Probe> {
    vec![
        Probe {
            tag: "string",
            php: "str_repeat($v, 1)",
            function: "str_repeat",
            arguments: |value| vec![value.clone(), Value::Int(1)],
        },
        Probe {
            tag: "int",
            php: "intdiv($v, 1)",
            function: "intdiv",
            arguments: |value| vec![value.clone(), Value::Int(1)],
        },
        Probe {
            tag: "float",
            php: "fdiv($v, 1.0)",
            function: "fdiv",
            arguments: |value| vec![value.clone(), Value::Float(1.0)],
        },
        Probe {
            tag: "bool",
            php: "array_key_first(array_reverse([1, 2], $v)) === 1",
            function: "array_reverse",
            arguments: |value| vec![list(vec![Value::Int(1), Value::Int(2)]), value.clone()],
        },
        Probe {
            tag: "array",
            php: "array_slice($v, 0, null, true)",
            function: "array_slice",
            arguments: |value| vec![value.clone(), Value::Int(0), Value::Null, Value::Bool(true)],
        },
        Probe {
            tag: "number",
            php: "abs($v)",
            function: "abs",
            arguments: |value| vec![value.clone()],
        },
        Probe {
            tag: "int-or-string",
            php: "array_column([$ROW], $v)",
            function: "array_column",
            arguments: |value| vec![Value::Array(Rc::new(column_row())), value.clone()],
        },
        // With nothing to search for, str_replace() returns its subject.
        Probe {
            tag: "array-or-string",
            php: "str_replace('', '', $v)",
            function: "str_replace",
            arguments: |value| vec![Value::from(""), Value::from(""), value.clone()],
        },
        Probe {
            tag: "object",
            php: "spl_object_id($v) > 0",
            function: "spl_object_id",
            arguments: |value| vec![value.clone()],
        },
        Probe {
            tag: "variadic",
            php: "count(array_merge([], $v))",
            function: "array_merge",
            arguments: |value| vec![list(vec![]), value.clone()],
        },
    ]
}

fn run(strict: bool) {
    let what = if strict {
        "arguments, strict_types"
    } else {
        "arguments, weak"
    };
    let values = values();
    let probes = probes();
    let registry = registry();

    let mut script = String::from("<?php\n");
    if strict {
        script.push_str("declare(strict_types=1);\n");
    }
    script.push_str(oracle::PHP_PREAMBLE);
    script.push_str(&format!(
        "$ROW = {};\n",
        oracle::php_literal(&Value::Array(Rc::new(column_row())))
    ));

    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for probe in &probes {
        for (source, value) in &values {
            script.push_str(&format!(
                "$v = {source}; run('{}', fn() => {});\n",
                probe.tag, probe.php
            ));
            labels.push(format!("{} with {source}", probe.php));

            let mut context = TestContext {
                strict_types: strict,
                ..TestContext::default()
            };
            let result = call(
                &registry,
                &mut context,
                probe.function,
                (probe.arguments)(value),
            );
            rust.push(oracle::encode_case(
                probe.tag,
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
    }

    let Some(php) = oracle::run_php(&what.replace([',', ' '], "-"), &script) else {
        eprintln!("php binary not found; skipping {what}");
        return;
    };
    oracle::compare(what, &labels, &php, &rust);
}

#[test]
fn weak_mode_matches_php() {
    run(false);
}

#[test]
fn strict_types_matches_php() {
    run(true);
}
