//! PHP 8.5 comparisons for INI values, CSV bytes and `round(2.5, 0, RoundingMode::HalfEven)`.

use std::fmt::Write;

use phera_ext_standard::Standard;
use phera_extension_api::{
    ArrayKey, Context, PhpArray, Registry, Value,
    testing::{TestContext, call, oracle},
};

#[test]
fn ini_scanner_matches_php() {
    let mut cases = Vec::new();
    let values = [
        "",
        "0",
        "01",
        "5.",
        ".5",
        "-00",
        "-3.5",
        "+2",
        "1e3",
        "01x",
        "YES",
        "none",
        "null",
        "falsehood",
        "9223372036854775808",
        "1 & 2 | 4",
        "(3 ^ 2) & ~0",
        "!!2",
        "literal trailing   ",
        "'single;quoted'",
        "\"double;quoted\"",
        "\"multi\nline\"",
        "foo # comment",
        "left; comment",
        "\"a\" \"b\"",
        "\"a\\\"b\\$c\"",
        "one\0extra=ignored",
        "\"unterminated",
        "${MISSING:-42}",
        "${MISSING:-${ALSO_MISSING:-value}}",
        "${MISSING",
        "${:-}",
        "$0",
    ];
    for (key, value, end) in values.into_iter().flat_map(|value| {
        ["key", "key[]", "key[name]", "8[]", "007[]"]
            .into_iter()
            .flat_map(move |key| ["", "\n"].into_iter().map(move |end| (key, value, end)))
    }) {
        for mode in 0..=2 {
            cases.push(vec![
                Value::from(format!("top=one\n[ first ]\n{key} = {value}{end}")),
                Value::Bool(true),
                Value::Int(mode),
            ]);
        }
    }
    for input in [
        b"[bad".as_slice(),
        b"true = yes",
        b"key = 1 |",
        b"key = a=b",
        b"key = (\"a\"",
        b"key[unused]\n",
        b"key=\"a\xff\xfe\"",
        b"\xef\xbb\xbfkey=2",
        b"",
    ] {
        for sections in [false, true] {
            for mode in [-1, 0, 1, 2, 3, i64::MAX] {
                cases.push(vec![
                    Value::from(input),
                    Value::Bool(sections),
                    Value::Int(mode),
                ]);
            }
        }
    }
    cases.extend([
        vec![],
        vec![Value::from(PhpArray::new())],
        vec![Value::from("a=1"), Value::from(PhpArray::new())],
        vec![Value::from("a=1"), Value::Null, Value::Null],
    ]);
    compare("parse_ini_string", cases, &[]);
}

#[test]
fn csv_scanner_matches_php() {
    let mut inputs: Vec<Vec<u8>> = [
        b"".as_slice(),
        b"\n",
        b"\r\n",
        b"\r\r\n",
        b",,",
        b"\"",
        b"\"\"\"",
        b"\"a\r\nb\",c\r\n",
        b"\"a\"\r,b",
        b"a\r,b\n",
        b" \t\"a\"\"b\" , d",
        b"\"a\\\"b\",c",
        b"a\xff\0b,c",
        b"\"a\\\r\n",
    ]
    .into_iter()
    .map(<[u8]>::to_vec)
    .collect();
    let alphabet = b"ax,b\t \";\\\r\n\0\xff";
    let mut seed = 0x431f5d57u64;
    for length in 0..32 {
        for _ in 0..5 {
            let mut input = Vec::new();
            for _ in 0..length {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                input.push(alphabet[((seed >> 32) as usize) % alphabet.len()]);
            }
            inputs.push(input);
        }
    }
    let mut cases = Vec::new();
    for input in inputs {
        cases.push(vec![Value::from(input.as_slice())]);
        for (separator, enclosure) in [
            (",", "\""),
            (";", "'"),
            ("\t", "\""),
            ("\0", "\""),
            ("\"", "\""),
        ] {
            for escape in ["", "\\", "\""] {
                cases.push(vec![
                    Value::from(input.as_slice()),
                    Value::from(separator),
                    Value::from(enclosure),
                    Value::from(escape),
                ]);
            }
        }
    }
    for invalid in ["", "xx", "é"] {
        cases.push(vec![Value::from("a,b"), Value::from(invalid)]);
        cases.push(vec![
            Value::from("a,b"),
            Value::from(","),
            Value::from(invalid),
        ]);
        cases.push(vec![
            Value::from("a,b"),
            Value::from(","),
            Value::from("\""),
            Value::from(invalid),
        ]);
    }
    cases.extend([
        vec![],
        vec![Value::Null],
        vec![Value::from(PhpArray::new())],
    ]);
    compare("str_getcsv", cases, &[]);
}

#[test]
fn rounding_enum_matches_php() {
    let names = [
        "HalfAwayFromZero",
        "HalfTowardsZero",
        "HalfEven",
        "HalfOdd",
        "TowardsZero",
        "AwayFromZero",
        "NegativeInfinity",
        "PositiveInfinity",
    ];
    let mut cases = Vec::new();
    let mut enums = Vec::new();
    let mut context = TestContext::default();
    for name in names {
        let mut properties = PhpArray::new();
        properties.insert(ArrayKey::str("name"), Value::from(name));
        enums.push(context.create_object("RoundingMode", properties).unwrap());
    }
    for value in [
        Value::Int(i64::MAX),
        Value::Int(i64::MIN),
        Value::Int(-55),
        Value::Float(-0.0),
        Value::Float(-1.955),
        Value::Float(0.285),
        Value::Float(2.5),
        Value::Float(-2.5),
        Value::Float(500.0),
        Value::Float(1e23),
        Value::Float(1e-23),
        Value::Float(f64::INFINITY),
        Value::Float(f64::NAN),
        Value::from("2.5"),
    ] {
        for precision in [i64::MIN, -309, -23, -3, -1, 0, 1, 3, 22, 23, 309, i64::MAX] {
            for mode in &enums {
                cases.push(vec![value.clone(), Value::Int(precision), mode.clone()]);
            }
        }
    }
    for mode in [
        Value::Int(99),
        Value::Float(4.5),
        Value::from("5"),
        Value::Bool(false),
        Value::Null,
        Value::from(PhpArray::new()),
    ] {
        cases.push(vec![Value::Float(2.5), Value::Int(0), mode]);
    }
    compare("round", cases, &names);
}

fn compare(function: &str, cases: Vec<Vec<Value>>, enum_names: &[&str]) {
    let mut registry = Registry::new();
    registry.load(&Standard);
    let mut script = format!("<?php\n{}\nsetlocale(LC_ALL, 'C');\n", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    for (number, arguments) in cases.into_iter().enumerate() {
        let args = arguments
            .iter()
            .map(|value| {
                if let Value::Object(object) = value
                    && let Some(Value::String(name)) = object.get(&ArrayKey::str("name"))
                    && let Some(case) = enum_names
                        .iter()
                        .find(|case| name.as_bytes() == case.as_bytes())
                {
                    return format!("RoundingMode::{case}");
                }
                oracle::php_literal(value)
            })
            .collect::<Vec<_>>()
            .join(", ");
        let label = format!("{function}({args})");
        writeln!(script, "run('{number}', fn() => {label});").unwrap();
        labels.push(label);
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, function, arguments);
        rust.push(oracle::encode_case(
            &number.to_string(),
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }
    if let Some(php) = oracle::run_php(&format!("phera-diagnostics-{function}"), &script) {
        oracle::compare(function, &labels, &php, &rust);
    }
}
