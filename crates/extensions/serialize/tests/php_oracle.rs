//! Compare results and warnings with PHP 8.5 over a deterministic input matrix.

use std::fmt::Write;
use std::rc::Rc;

use phera_ext_serialize::Serialize;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{ArrayKey, PhpArray, Registry, Value};

#[test]
fn unserialize_php_matrix() {
    let mut inputs: Vec<Vec<u8>> = [
        &b""[..],
        b"N;",
        b"N;\0",
        b"b:1;",
        b"b:0;",
        b"b:01;",
        b"b:+1;",
        b"}",
        b"i:-9223372036854775808;",
        b"i:9223372036854775808;",
        b"i:18446744073709551615;",
        b"i:-999999999999999999999;",
        b"i:+0000000000000000000000004;",
        b"i:+;",
        b"i:9",
        b"d:INF;",
        b"d:-INF;",
        b"d:+INF;",
        b"d:NAN;",
        b"d:NaN;",
        b"d:-0;",
        b"d:1.;",
        b"d:+.1e-4;",
        b"d:-1.E+99999999;",
        b"d:.e1;",
        b"d:1e+;",
        b"d:4.9406564584124654e-324;",
        b"s:3:\"x\0\xff\";",
        b"s:0:\"\";",
        b"s:01:\"a\";",
        b"s:+1:\"a\";",
        b"s:10:\"a\";",
        b"s:18446744073709551616:\"\";",
        b"s:1:\"x",
        b"s:1:\"x\"",
        b"s:1:\"xy\";",
        b"S:1:\"\\00\";",
        b"S:2:\"\\fFa\";",
        b"S:1:\"\\aG\";",
        b"s:1:\"\\00\";",
        b"a:0:{}",
        b"a:1:{i:0;N;}",
        b"a:01:{i:0;a:0:{}}",
        b"a:2:{i:0;a:1:{i:1;N;}i:2;N;}",
        b"a:1:{b:1;N;}",
        b"a:1:{a:0:{}N;}",
        b"a:1:{s:2:\"-0\";d:0;}",
        b"a:1:{s:1:\"0\";d:0;}",
        b"a:2:{i:0;i:1;i:0;i:2;}",
        b"a:1:{i:0;}  ",
        b"a:2000000000:{i:0;N;}",
        b"a:+1:{i:0;N;}",
        b"a:18446744073709551615:{}",
        b"R:0;",
        b"R:1;",
        b"r:1;",
        b"r:+1;",
        b"O:4:\"Gone\":0:{}",
        b"O:4:\"Gone\"::{}",
        b"O:4:\"Gone\":+0:{}",
        b"O:4:\"Gone\":0",
        b"O:4:\"Gone\":500:{}",
        b"O:4:\"Gone\":-1:{}",
        b"O:4:\"Gone\":2:{}",
        b"O:4:\"Gone\":+{}",
        b"O:4:\"Gone\":1:{i:0;N;}",
        b"O:4:\"Gone\":1:{s:1:\"p\";N;}",
        b"O:4:\"Go-e\":0:{}",
        b"C:4:\"Gone\":0:{}",
        b"C:4:\"Gone\":-1:{}",
        b"C:4:\"Gone\":6:{abc}",
        b"C:4:\"Gone\":0:{x}",
        b"C:3:\"XYZ\":18446744075857035259:{}",
        b"E:6:\"Gone:A\";",
        b"E:4:\"Gone\";",
        b"E:1:\"a\";",
        b"E:0:\"\";",
    ]
    .into_iter()
    .map(<[u8]>::to_vec)
    .collect();
    let seeds = inputs.clone();
    let mut random = 0x4790e9d145cc83a1u64;
    let mut draw = || {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        random as usize
    };
    for _ in 0..300 {
        let mut input = seeds[draw() % seeds.len()].clone();
        let position = draw() % (input.len() + 1);
        match draw() % 3 {
            0 => input.truncate(position),
            1 => input.insert(position, draw() as u8),
            _ if position < input.len() => input[position] = draw() as u8,
            _ => {}
        }
        inputs.push(input);
    }
    let options = [
        ("[]", options([])),
        (
            "['allowed_classes' => false]",
            options([("allowed_classes", Value::Bool(false))]),
        ),
        (
            "['allowed_classes' => true]",
            options([("allowed_classes", Value::Bool(true))]),
        ),
        (
            "['allowed_classes' => ['gone']]",
            options([("allowed_classes", list([Value::from("gone")]))]),
        ),
        (
            "['allowed_classes' => ['x y']]",
            options([("allowed_classes", list([Value::from("x y")]))]),
        ),
        (
            "['allowed_classes' => [1]]",
            options([("allowed_classes", list([Value::Int(1)]))]),
        ),
        (
            "['allowed_classes' => 0]",
            options([("allowed_classes", Value::Int(0))]),
        ),
        (
            "['max_depth' => 1]",
            options([("max_depth", Value::Int(1))]),
        ),
        (
            "['max_depth' => 0]",
            options([("max_depth", Value::Int(0))]),
        ),
        (
            "['max_depth' => -1]",
            options([("max_depth", Value::Int(-1))]),
        ),
        (
            "['max_depth' => '1']",
            options([("max_depth", Value::from("1"))]),
        ),
    ];
    let mut cases = Cases::new();
    for input in inputs {
        let hex = to_hex(&input);
        for (expression, option) in &options {
            cases.push(
                "unserialize",
                &format!("hex2bin('{hex}'), {expression}"),
                vec![Value::from(input.clone()), option.clone()],
            );
        }
    }
    cases.check("serialize-unserialize-matrix");
}

#[test]
fn serialize_php_matrix() {
    let mut cases = Cases::new();
    for (expression, value) in [
        ("null", Value::Null),
        ("true", Value::Bool(true)),
        ("false", Value::Bool(false)),
        ("PHP_INT_MAX", Value::Int(i64::MAX)),
        ("PHP_INT_MIN", Value::Int(i64::MIN)),
        ("NAN", Value::Float(f64::NAN)),
        ("INF", Value::Float(f64::INFINITY)),
        ("-INF", Value::Float(f64::NEG_INFINITY)),
        ("hex2bin('00ff000a')", Value::from(b"\0\xff\0\n".to_vec())),
        (
            "[null, 1, false]",
            list([Value::Null, Value::Int(1), Value::Bool(false)]),
        ),
        ("[]", list([])),
    ] {
        cases.push("serialize", expression, vec![value]);
    }
    let mut bits = 0xc3dc2db218da258du64;
    cases.push("serialize", "-0.0", vec![Value::Float(-0.0)]);
    for whole in [-274877906943.0, -7.0, -1.0, 0.0, 42.0, 274877906943.0] {
        for sixteenths in 0..16 {
            let value = whole + f64::from(sixteenths) / 16.0;
            cases.push(
                "serialize",
                &format!("{value:?}"),
                vec![Value::Float(value)],
            );
        }
    }
    for _ in 0..8192 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let expression = format!("unpack('E', hex2bin('{bits:016x}'))[1]");
        cases.push(
            "serialize",
            &expression,
            vec![Value::Float(f64::from_bits(bits))],
        );
    }
    for exponent in 1..2047 {
        let center = exponent << 52;
        for bits in [center - 1, center, center + 1] {
            let expression = format!("unpack('E', hex2bin('{bits:016x}'))[1]");
            cases.push(
                "serialize",
                &expression,
                vec![Value::Float(f64::from_bits(bits))],
            );
        }
    }
    for (expression, args) in [("", vec![]), ("1, 2", vec![Value::Int(1), Value::Int(2)])] {
        cases.push("serialize", expression, args);
    }
    for (expression, args) in [
        ("", vec![]),
        ("null", vec![Value::Null]),
        ("1", vec![Value::Int(1)]),
        ("[]", vec![list([])]),
        ("'N;', true", vec![Value::from("N;"), Value::Bool(true)]),
        (
            "'N;', [], 0",
            vec![Value::from("N;"), list([]), Value::Int(0)],
        ),
    ] {
        cases.push("unserialize", expression, args);
    }
    cases.check("serialize-serialize-matrix");
}

fn options<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let array: PhpArray = entries
        .into_iter()
        .map(|(key, value)| (ArrayKey::str(key), value))
        .collect();
    Value::Array(Rc::new(array))
}

fn list<const N: usize>(entries: [Value; N]) -> Value {
    let array: PhpArray = entries
        .into_iter()
        .enumerate()
        .map(|(key, value)| (ArrayKey::Int(key as i64), value))
        .collect();
    Value::Array(Rc::new(array))
}

fn to_hex(input: &[u8]) -> String {
    let mut hex = String::with_capacity(input.len() * 2);
    for byte in input {
        write!(hex, "{byte:02x}").expect("write to string");
    }
    hex
}

struct Cases {
    registry: Registry,
    script: String,
    labels: Vec<String>,
    lines: Vec<String>,
}

impl Cases {
    fn new() -> Self {
        let mut registry = Registry::new();
        registry.load(&Serialize);
        Self {
            registry,
            script: format!("<?php\n{}", oracle::PHP_PREAMBLE),
            labels: Vec::new(),
            lines: Vec::new(),
        }
    }

    fn push(&mut self, function: &str, expression: &str, arguments: Vec<Value>) {
        let tag = self.lines.len().to_string();
        let label = format!("{function}({expression})");
        writeln!(&mut self.script, "run('{tag}', fn() => {label});").expect("write to string");
        let mut context = TestContext::default();
        let result = call(&self.registry, &mut context, function, arguments);
        if let Ok(Value::Object(object)) = &result {
            // PHP oracle enc() inspects (array) $object, which normalizes numeric keys.
            let properties = object
                .to_array()
                .iter()
                .map(|(key, value)| {
                    let key = match key {
                        ArrayKey::String(key) => ArrayKey::from_value(&Value::String(key))
                            .expect("string keys can be normalized"),
                        key => key,
                    };
                    (key, value.clone())
                })
                .collect();
            object.update(|storage| *storage = properties);
        }
        self.labels.push(label);
        self.lines.push(oracle::encode_case(
            &tag,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }

    fn check(self, name: &str) {
        if let Some(php) = oracle::run_php(name, &self.script) {
            println!("{name}: {} compared calls", self.lines.len());
            oracle::compare(name, &self.labels, &php, &self.lines);
        }
    }
}
