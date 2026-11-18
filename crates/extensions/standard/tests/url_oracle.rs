//! URL parsing and form encoding compared byte for byte with PHP.

use std::{cell::RefCell, fmt::Write, rc::Rc};

use phera_ext_standard::Standard;
use phera_extension_api::{
    ArrayKey, PhpArray, Registry, Value,
    testing::{TestContext, call, oracle},
};

#[test]
fn url_and_query_calls_match_php() {
    let mut registry = Registry::new();
    registry.load(&Standard);
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut actual = Vec::new();
    let mut case = |name: &str, arguments: Vec<Value>| {
        let tag = format!("q{}", labels.len());
        let php_arguments = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>();
        let parse = name == "parse_str" && arguments.len() == 2;
        if parse {
            writeln!(
                script,
                "run('{tag}', function () {{ $out = {}; $ret = parse_str({}, $out); \
                 return [$ret, $out]; }});",
                php_arguments[1], php_arguments[0]
            )
            .unwrap();
        } else {
            writeln!(
                script,
                "run('{tag}', fn () => {name}({}));",
                php_arguments.join(", ")
            )
            .unwrap();
        }
        labels.push(format!("{name}({})", php_arguments.join(", ")));
        let mut arguments = arguments;
        let reference = parse.then(|| Rc::new(RefCell::new(arguments[1].clone())));
        if let Some(reference) = &reference {
            arguments[1] = Value::Reference(reference.clone());
        }
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, name, arguments).map(|value| {
            if let Some(reference) = reference {
                let mut result = PhpArray::new();
                result.push(value).unwrap();
                result.push(reference.borrow().clone()).unwrap();
                Value::from(result)
            } else {
                value
            }
        });
        actual.push(oracle::encode_case(
            &tag,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    };

    for left in [
        b"".as_slice(),
        b"/",
        b"//",
        b":",
        b"x:",
        b"http://",
        b"file://",
        b"user:pass@",
        b"http://user@host:",
        b"http://[::1]:",
        b"/with\0bad\xff?",
        b"./a:",
    ] {
        for right in [
            b"".as_slice(),
            b"/a?#",
            b"?a#b?c",
            b"#",
            b"host",
            b"00",
            b"65535",
            b"65536",
            b"123456",
            b"1\0xy",
            b" 12!",
            b"-1",
            b"a:2",
            b"/C:/hello",
            b"foo@bar@host",
            b"\x01\x7f\xff",
        ] {
            let url = Value::from([left, right].concat());
            case("parse_url", vec![url.clone()]);
            for component in [-10, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, i64::MAX] {
                case("parse_url", vec![url.clone(), Value::Int(component)]);
            }
        }
    }
    for key in [
        "",
        "a",
        "01",
        "-1",
        "a b.",
        " a",
        "%09a",
        "%00a",
        "a%00b",
        "a[b]",
        "a[]",
        "a[x][y][]",
        "a[ ",
        "a[b.c]",
        "a[][",
        "a[x][",
        "..Host-x",
        "a[__Host-x]",
        "a[9223372036854775807][]",
    ] {
        for value in ["", "2=3", "%zz+", "%FF%00", "abc\0discard=yes", "x&&c=3"] {
            case(
                "parse_str",
                vec![Value::from(format!("a=0&{key}={value}")), Value::Int(9)],
            );
        }
    }
    for depth in [63, 64, 65, 129] {
        case(
            "parse_str",
            vec![
                Value::from(format!("a=1&a{}=deep&b=2", "[x]".repeat(depth))),
                Value::Null,
            ],
        );
    }
    case(
        "parse_str",
        vec![Value::from("a=1&".repeat(1001)), Value::Null],
    );

    let mut nested = PhpArray::new();
    for (key, value) in [
        (
            ArrayKey::str("a[[]+& "),
            Value::from(b"\x00\xff~+ ".as_slice()),
        ),
        (ArrayKey::int(-4), Value::Float(1e23)),
        (ArrayKey::int(0), Value::Bool(false)),
        (ArrayKey::str("skip"), Value::Null),
    ] {
        nested.insert(key, value);
    }
    let mut outer = nested.clone();
    outer.insert(ArrayKey::int(14), Value::from(nested));
    for data in [
        Value::from(PhpArray::new()),
        Value::from(outer),
        Value::Int(2),
        Value::Null,
    ] {
        for prefix in ["", "index_", "~ &\0"] {
            for separator in [Value::Null, Value::from(""), Value::from(";\0")] {
                for encoding in [1, 2, 3, 4_294_967_298, i64::MAX] {
                    case(
                        "http_build_query",
                        vec![
                            data.clone(),
                            Value::from(prefix),
                            separator.clone(),
                            Value::Int(encoding),
                        ],
                    );
                }
            }
        }
    }
    case("parse_url", Vec::new());
    case("parse_url", vec![Value::from(PhpArray::new())]);
    case("parse_url", vec![Value::from("x"), Value::from("port")]);
    case("parse_str", vec![Value::from("")]);
    case("parse_str", vec![Value::from(PhpArray::new()), Value::Null]);
    case("http_build_query", Vec::new());

    let Some(php) = oracle::run_php("web-basics-url", &script) else {
        eprintln!("php binary not found; skipping");
        return;
    };
    oracle::compare("URL and query functions", &labels, &php, &actual);
}
