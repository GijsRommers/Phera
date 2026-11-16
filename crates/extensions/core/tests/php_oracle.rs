//! Every function of this extension, run over a matrix of values in real PHP
//! and here, and compared line by line.

use std::cell::RefCell;
use std::rc::Rc;

use phera_ext_core::Core;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::value::{ClassHandle, Object};
use phera_extension_api::{ArrayKey, Extension, NativeResult, PhpArray, Registry, Value};

struct StdClass;

impl ClassHandle for StdClass {
    fn name(&self) -> &str {
        "stdClass"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn registry() -> Registry {
    let mut registry = Registry::new();
    Core.register(&mut registry);
    registry
}

fn list(items: Vec<Value>) -> Value {
    let mut array = PhpArray::new();
    for item in items {
        array.push(item).expect("push");
    }
    Value::from(array)
}

fn map(entries: Vec<(ArrayKey, Value)>) -> Value {
    Value::from(entries.into_iter().collect::<PhpArray>())
}

fn object(properties: Vec<(&str, Value)>) -> Rc<Object> {
    // PHP's ids depend on what else is alive; the PHP side rewrites them to 1.
    let object = Object::new(1, Rc::new(StdClass));
    for (name, value) in properties {
        object
            .properties
            .borrow_mut()
            .insert(ArrayKey::str(name), value);
    }
    Rc::new(object)
}

/// A value to test with: PHP statements that leave it in `$v`, and the same
/// value built in Rust.
struct Case {
    php: String,
    value: Value,
    /// Other holders of references inside the value, which PHP keeps alive
    /// in variables. Dropping them would change what `var_dump` marks as `&`.
    _alive: Vec<Rc<RefCell<Value>>>,
    /// The value contains itself, so it cannot be encoded as a result.
    cyclic: bool,
}

fn plain(value: Value) -> Case {
    Case {
        php: format!("$v = {};", oracle::php_literal(&value)),
        value,
        _alive: Vec::new(),
        cyclic: false,
    }
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    cases.push(plain(Value::Null));
    cases.push(plain(Value::Bool(true)));
    cases.push(plain(Value::Bool(false)));
    for int in [0, 1, -1, 42, 1000000, i64::MAX, i64::MIN] {
        cases.push(plain(Value::Int(int)));
    }
    for float in [
        0.0,
        -0.0,
        1.0,
        1.5,
        -1.5,
        0.1,
        0.1 + 0.2,
        100.0,
        1e15,
        1e21,
        1e25,
        -1e25,
        1e100,
        1e-7,
        123456789.12345678,
        9.223372036854776e18,
        f64::MAX,
        f64::MIN_POSITIVE,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        cases.push(plain(Value::Float(float)));
    }
    for string in [
        "",
        "0",
        "1",
        "abc",
        "12",
        " 12",
        "12 ",
        "12abc",
        "1.5",
        "1e3",
        "-0",
        "0x1A",
        "0b11",
        "012",
        "0o17",
        "-0x1a",
        "+0b101",
        " \t42",
        "9223372036854775807",
        "9223372036854775808",
        "-9223372036854775809",
        "99999999999999999999999",
        ".5",
        "zz",
        "Z9",
        "it's",
        "back\\slash",
        "a\0b",
        "\0",
        "line\nbreak",
        "tab\there",
        "\u{e9}t\u{e9}",
        "null",
        "true",
    ] {
        cases.push(plain(Value::from(string)));
    }
    cases.push(plain(Value::from(vec![0xff_u8, 0xfe, 0x00, 0x27])));

    cases.push(plain(list(vec![])));
    cases.push(plain(list(vec![
        Value::Int(1),
        Value::Int(2),
        Value::Int(3),
    ])));
    cases.push(plain(list(vec![
        Value::Null,
        Value::Bool(true),
        Value::Bool(false),
        Value::Float(1.0),
        Value::Float(-0.0),
        Value::from("s"),
        Value::Int(i64::MIN),
    ])));
    cases.push(plain(map(vec![
        (ArrayKey::str("a"), Value::Int(1)),
        (ArrayKey::int(-5), Value::from("neg")),
        (ArrayKey::str("it's"), Value::from("q")),
        (ArrayKey::str(""), Value::Null),
        (ArrayKey::str("a\0b"), Value::Float(0.5)),
        (ArrayKey::int(7), list(vec![])),
    ])));
    cases.push(plain(list(vec![
        list(vec![
            Value::Int(1),
            list(vec![Value::Int(2), list(vec![Value::Int(3)])]),
        ]),
        map(vec![
            (ArrayKey::str("k"), list(vec![list(vec![])])),
            (ArrayKey::str("z"), Value::from("end")),
        ]),
        Value::Int(9),
    ])));

    // A reference that a variable still shares, and one nobody else holds.
    let shared = Rc::new(RefCell::new(Value::Int(1)));
    cases.push(Case {
        php: "$x = 1; $v = [1, 'k' => &$x, 'n' => [2]];".to_string(),
        value: map(vec![
            (ArrayKey::int(0), Value::Int(1)),
            (ArrayKey::str("k"), Value::Reference(shared.clone())),
            (ArrayKey::str("n"), list(vec![Value::Int(2)])),
        ]),
        _alive: vec![shared],
        cyclic: false,
    });
    cases.push(Case {
        php: "$v = [1, 2]; $y = &$v[0]; unset($y);".to_string(),
        value: list(vec![
            Value::Reference(Rc::new(RefCell::new(Value::Int(1)))),
            Value::Int(2),
        ]),
        _alive: Vec::new(),
        cyclic: false,
    });

    // Two elements bound to the same array.
    let inner = Rc::new(RefCell::new(list(vec![Value::Int(1)])));
    cases.push(Case {
        php: "$v = [[1]]; $v[1] = &$v[0];".to_string(),
        value: list(vec![
            Value::Reference(inner.clone()),
            Value::Reference(inner),
        ]),
        _alive: Vec::new(),
        cyclic: false,
    });

    // An array that contains itself through a reference.
    let cell = Rc::new(RefCell::new(Value::Null));
    let cyclic = list(vec![Value::Int(1), Value::Reference(cell.clone())]);
    *cell.borrow_mut() = cyclic.clone();
    cases.push(Case {
        php: "$v = [1]; $v[1] = &$v;".to_string(),
        value: cyclic,
        _alive: vec![cell],
        cyclic: true,
    });

    // The same array twice, side by side: not recursion.
    let twin = list(vec![Value::Int(1), list(vec![Value::Int(2)])]);
    cases.push(Case {
        php: "$t = [1, [2]]; $v = [$t, $t];".to_string(),
        value: list(vec![twin.clone(), twin]),
        _alive: Vec::new(),
        cyclic: false,
    });

    // Objects: properties, nesting, and one that contains itself.
    cases.push(Case {
        php: "$v = new stdClass;".to_string(),
        value: Value::Object(object(vec![])),
        _alive: Vec::new(),
        cyclic: false,
    });
    cases.push(Case {
        php: "$v = new stdClass; $v->a = 1; $v->{'b c'} = [1, 'x' => null]; $v->n = new stdClass; $v->n->deep = 1.0;"
            .to_string(),
        value: Value::Object(object(vec![
            ("a", Value::Int(1)),
            (
                "b c",
                map(vec![
                    (ArrayKey::int(0), Value::Int(1)),
                    (ArrayKey::str("x"), Value::Null),
                ]),
            ),
            ("n", Value::Object(object(vec![("deep", Value::Float(1.0))]))),
        ])),
        _alive: Vec::new(),
        cyclic: false,
    });
    let selfish = object(vec![("a", Value::Int(1))]);
    selfish
        .properties
        .borrow_mut()
        .insert(ArrayKey::str("self"), Value::Object(selfish.clone()));
    cases.push(Case {
        php: "$v = new stdClass; $v->a = 1; $v->self = $v;".to_string(),
        value: Value::Object(selfish),
        _alive: Vec::new(),
        cyclic: true,
    });
    cases.push(Case {
        php: "$v = [new stdClass, 'o' => new stdClass];".to_string(),
        value: map(vec![
            (ArrayKey::int(0), Value::Object(object(vec![]))),
            (ArrayKey::str("o"), Value::Object(object(vec![]))),
        ]),
        _alive: Vec::new(),
        cyclic: false,
    });
    cases
}

/// One call per line: PHP source over `$v`, and the Rust arguments.
struct Call {
    php: String,
    function: &'static str,
    arguments: Vec<Value>,
}

fn calls(value: &Value) -> Vec<Call> {
    let one = |function: &'static str| Call {
        php: format!("{function}($v)"),
        function,
        arguments: vec![value.clone()],
    };
    let two = |function: &'static str, php: &str, second: Value| Call {
        php: format!("{function}($v, {php})"),
        function,
        arguments: vec![value.clone(), second],
    };
    let mut calls = vec![
        one("var_dump"),
        one("print_r"),
        two("print_r", "true", Value::Bool(true)),
        one("var_export"),
        two("var_export", "true", Value::Bool(true)),
        Call {
            php: "var_dump($v, 1, $v)".to_string(),
            function: "var_dump",
            arguments: vec![value.clone(), Value::Int(1), value.clone()],
        },
    ];
    for function in [
        "gettype",
        "get_debug_type",
        "is_null",
        "is_bool",
        "is_int",
        "is_integer",
        "is_long",
        "is_float",
        "is_double",
        "is_string",
        "is_array",
        "is_object",
        "is_resource",
        "is_numeric",
        "is_scalar",
        "is_iterable",
        "is_countable",
        "intval",
        "floatval",
        "doubleval",
        "strval",
        "boolval",
        "count",
        "sizeof",
    ] {
        calls.push(one(function));
    }
    calls.push(two("count", "COUNT_RECURSIVE", Value::Int(1)));
    calls.push(two("count", "2", Value::Int(2)));
    if value.is_string() {
        for base in [0, 2, 3, 8, 10, 16, 36, 1, 37, -5] {
            calls.push(two("intval", &base.to_string(), Value::Int(base)));
        }
    }
    calls
}

#[test]
fn functions_match_php() {
    let what = "core functions";
    let registry = registry();
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    // Object ids depend on what else PHP has allocated.
    script.push_str(
        "function same_ids($f) { ob_start(); $r = $f(); \
         echo preg_replace('/\\)#\\d+ \\(/', ')#1 (', ob_get_clean()); return $r; }\n",
    );

    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for case in cases() {
        for item in calls(&case.value) {
            script.push_str(&format!(
                "run('{}', function () {{ {} return same_ids(fn() => {}); }});\n",
                item.function, case.php, item.php
            ));
            labels.push(format!("{} after {}", item.php, case.php));
            let mut context = TestContext::default();
            let result = call(&registry, &mut context, item.function, item.arguments);
            rust.push(oracle::encode_case(
                item.function,
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
    }

    let Some(php) = oracle::run_php("core-functions", &script) else {
        eprintln!("php binary not found; skipping {what}");
        return;
    };
    oracle::compare(what, &labels, &php, &rust);
}

#[test]
fn settype_matches_php() {
    let what = "settype";
    let registry = registry();
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);

    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for case in cases().into_iter().filter(|case| !case.cyclic) {
        // Converting to an object needs classes, which come later.
        for target in [
            "int", "integer", "float", "double", "string", "array", "bool", "boolean", "null",
            "NULL", "Int", "resource", "nonsense", "",
        ] {
            script.push_str(&format!(
                "run('settype', function () {{ {} $r = settype($v, '{target}'); return [$r, $v]; }});\n",
                case.php
            ));
            labels.push(format!("settype($v, '{target}') after {}", case.php));

            let variable = Rc::new(RefCell::new(case.value.clone()));
            let mut context = TestContext::default();
            let result: NativeResult = call(
                &registry,
                &mut context,
                "settype",
                vec![Value::Reference(variable.clone()), Value::from(target)],
            )
            .map(|returned| list(vec![returned, variable.borrow().clone()]));
            rust.push(oracle::encode_case(
                "settype",
                &result,
                &context.output,
                &context.diagnostics,
            ));
        }
    }

    let Some(php) = oracle::run_php("core-settype", &script) else {
        eprintln!("php binary not found; skipping {what}");
        return;
    };
    oracle::compare(what, &labels, &php, &rust);
}

#[test]
fn constants_match_php() {
    let registry = registry();
    let mut names: Vec<&str> = registry.constants().map(|(name, _)| name).collect();
    names.sort_unstable();
    // Phera reports its own version, and E_STRICT is deprecated to read.
    names.retain(|name| !name.contains("VERSION") && *name != "E_STRICT");

    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    let mut rust = Vec::new();
    let mut labels = Vec::new();
    for name in &names {
        script.push_str(&format!("run('{name}', fn() => {name});\n"));
        labels.push(name.to_string());
        let value = registry.get_constant(name).expect("constant").to_value();
        rust.push(format!("{name}={}", oracle::encode(&value)));
    }
    let Some(php) = oracle::run_php("core-constants", &script) else {
        eprintln!("php binary not found; skipping constants");
        return;
    };
    oracle::compare("core constants", &labels, &php, &rust);
}

/// 200,000 levels deep: a recursive count would overflow the stack here.
/// (The printing functions have the same guarantee, tested next to the
/// walker, because their output for such a value is far too large to build.)
#[test]
fn count_recursive_handles_deep_nesting() {
    let registry = registry();
    let mut value = list(vec![Value::Int(1)]);
    for _ in 0..200_000 {
        value = list(vec![value]);
    }
    let mut context = TestContext::default();
    let counted = call(&registry, &mut context, "count", vec![value, Value::Int(1)]);
    assert_eq!(counted.expect("count"), Value::Int(200_001));
}
