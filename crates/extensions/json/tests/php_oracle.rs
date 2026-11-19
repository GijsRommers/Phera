//! Deterministic differential coverage against PHP 8.5 in a single oracle process.
use phera_ext_json::Json;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
mod support;
use support::{Matrix, extra_cases, list, random};
#[test]
fn php_matrix() {
    let mut matrix = Matrix::new();
    let mut seed = 0x8ab8ee551322u64;
    let flagset = [
        0,
        1,
        2,
        4,
        8,
        16,
        32,
        64,
        128,
        256,
        512,
        1024,
        2048,
        1 << 20,
        1 << 21,
        1 << 22,
    ];
    for index in 0..12000 {
        let value = Value::Float(f64::from_bits(random(&mut seed)));
        matrix.case(
            "json_encode",
            vec![value, Value::Int(flagset[index % flagset.len()])],
        );
    }
    let strings: Vec<Vec<u8>> = (0..=255)
        .map(|byte| vec![byte])
        .chain([
            "é€😀\u{2028}\u{2029}<>&'\"/\\".as_bytes().to_vec(),
            vec![0xe1, 0x80],
            vec![0xed, 0xa0, 0x80],
            vec![0xf4, 0x90, 0x80, 0x80],
            b" +001.20e3 ".to_vec(),
            b"1e999".to_vec(),
        ])
        .collect();
    for bytes in &strings {
        for &flags in &flagset {
            matrix.case(
                "json_encode",
                vec![Value::from(bytes.clone()), Value::Int(flags)],
            );
        }
    }
    for index in 0..12000 {
        let number = random(&mut seed) as i64;
        let value = list(vec![
            Value::Int(number),
            Value::from(strings[index % strings.len()].clone()),
            list(vec![Value::Bool(true)]),
        ]);
        matrix.case(
            "json_encode",
            vec![
                value,
                Value::Int((random(&mut seed) & 0x700fff) as i64),
                Value::Int((index % 5) as i64),
            ],
        );
    }
    let corpus = [
        "null",
        "true",
        "false",
        "0",
        "-0",
        "1.0",
        "1e999",
        "-1e-999",
        "9223372036854775808",
        "[]",
        "{}",
        "[1,2,3]",
        "{\"a\":1,\"a\":2,\"0\":3,\"-0\":4}",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "\"\\ud800\\udc00\"",
        "\"\\u0000\"",
        "\"\\x\"",
        "[}",
        "{]",
        "[1,]",
        "{\"a\":}",
        "\"abc",
        "",
        " ",
        "01",
        "1.",
        "1e+",
        "[[]]",
        "\"a\\n\\t\\r\\b\\f\\/\\\\\\\"é😀\"",
        "{\"\\u0000a\":1}",
    ];
    for index in 0..72000 {
        let mut bytes = if index % 2 == 0 {
            corpus[index % corpus.len()].as_bytes().to_vec()
        } else {
            format!(
                "[{},{{\"key{}\":[true,null,\"value{}\"]}}]",
                random(&mut seed) as i64,
                index,
                index
            )
            .into_bytes()
        };
        if index % 4 != 0 && !bytes.is_empty() {
            let position = random(&mut seed) as usize % bytes.len();
            match index % 3 {
                0 => {
                    bytes.remove(position);
                }
                1 => bytes.insert(position, random(&mut seed) as u8),
                _ => {
                    let other = random(&mut seed) as usize % bytes.len();
                    bytes.swap(position, other);
                }
            }
        }
        let flags = [0, 2, 1 << 20, 1 << 21, 3 << 20, 1 << 22][index % 6];
        matrix.case(
            "json_decode",
            vec![
                Value::from(bytes.clone()),
                Value::Bool(true),
                Value::Int((index % 7 + 1) as i64),
                Value::Int(flags),
            ],
        );
        if index % 4 == 0 {
            matrix.case(
                "json_validate",
                vec![
                    Value::from(bytes),
                    Value::Int(512),
                    Value::Int(if index % 8 == 0 { 0 } else { 1 << 20 }),
                ],
            );
        }
    }
    for line in include_str!("phpt_strings.hex").lines() {
        let bytes: Vec<u8> = line
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect();
        for flags in [0, 2, 1 << 20, 1 << 21, 1 << 22] {
            matrix.case(
                "json_decode",
                vec![
                    Value::from(bytes.clone()),
                    Value::Bool(true),
                    Value::Int(512),
                    Value::Int(flags),
                ],
            );
        }
        matrix.case("json_validate", vec![Value::from(bytes)]);
    }
    extra_cases(&mut matrix);
    support::maps_and_bytes(&mut matrix);
    let php = oracle::run_php("json-matrix", &matrix.script).expect("PHP 8.5 required");
    oracle::compare("JSON matrix", &matrix.labels, &php, &matrix.rust);
}
#[test]
fn million_levels() {
    let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
    Json.register(&mut registry);
    let mut context = TestContext::default();
    let depth = 1_000_000;
    let text = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
    let decoded = call(
        &registry,
        &mut context,
        "json_decode",
        vec![
            Value::from(text.clone()),
            Value::Bool(true),
            Value::Int(depth as i64 + 1),
        ],
    )
    .unwrap();
    let encoded = call(
        &registry,
        &mut context,
        "json_encode",
        vec![decoded, Value::Int(0), Value::Int(depth as i64)],
    )
    .unwrap();
    assert_eq!(encoded, Value::from(text));
}
