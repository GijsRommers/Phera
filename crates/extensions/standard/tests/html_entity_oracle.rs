//! Compare PHP's entity names, numeric references, and ordered translation tables.

use std::fmt::Write;

use phera_ext_standard::Standard;
use phera_extension_api::{
    Registry, Value,
    testing::{TestContext, call, oracle},
};

#[test]
fn entity_functions_match_php() {
    let mut registry = Registry::new();
    registry.load(&Standard);
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut actual = Vec::new();
    let mut case = |function: &str, arguments: Vec<Value>| {
        let tag = format!("e{}", labels.len());
        let php_arguments = arguments
            .iter()
            .map(oracle::php_literal)
            .collect::<Vec<_>>()
            .join(", ");
        writeln!(
            script,
            "run('{tag}', fn () => {function}({php_arguments}));"
        )
        .unwrap();
        labels.push(format!("{function}({php_arguments})"));
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, function, arguments);
        actual.push(oracle::encode_case(
            &tag,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    };

    let names = include_str!("../src/strings/html/tables/html5-names.tsv")
        .lines()
        .map(|line| line.split_once('\t').unwrap().0)
        .chain(
            include_str!("../src/strings/html/tables/html4-names.tsv")
                .lines()
                .map(|line| line.split_once('\t').unwrap().0),
        )
        .collect::<std::collections::BTreeSet<_>>();
    for name in names {
        for doctype in [0, 16, 32, 48] {
            case(
                "html_entity_decode",
                vec![Value::from(format!("&{name};")), Value::Int(doctype | 3)],
            );
        }
    }

    let numeric = (0..=0x100)
        .chain(0x390..=0x460)
        .chain([
            0x152, 0x153, 0x178, 0x20ac, 0x2116, 0x20d2, 0xd7ff, 0xd800, 0xdfff, 0xe000, 0xfdcf,
            0xfdd0, 0xfdef, 0xfdf0, 0xfffd, 0xfffe, 0xffff, 0x1f680, 0x10fffd, 0x10fffe, 0x10ffff,
            0x110000,
        ])
        .map(|code| format!("&#x{code:x};/&#{code};"))
        .collect::<Vec<_>>()
        .join("|");
    let mixed = b"\xff&copy;\0&apos;&quot;&#039;&#34;&amp;amp;&lt;&gt;&fjlig;&nGt;&Tab;\
                  &#x0X41;&#X00041;&#0;&#x;&#-3;&#x-3;&#167\0;&&amp;&lt &lt;"
        .as_slice();

    for charset in [
        "",
        "UTF-8",
        "ISO-8859-1",
        "ISO-8859-15",
        "CP1252",
        "CP1251",
        "KOI8-R",
        "ISO-8859-5",
        "CP866",
        "MacRoman",
        "BIG5",
        "BIG5-HKSCS",
        "GB2312",
        "SJIS",
        "EUC-JP",
        "x-missing",
        "uTf-8\0ignored",
    ] {
        for doctype in [0, 16, 32, 48] {
            for quotes in 0..=3 {
                let flags = doctype | quotes | 8 | 128;
                for variant in [0, 1] {
                    case(
                        "get_html_translation_table",
                        vec![Value::Int(variant), Value::Int(flags), Value::from(charset)],
                    );
                }
                case(
                    "html_entity_decode",
                    vec![Value::from(mixed), Value::Int(flags), Value::from(charset)],
                );
            }
            case(
                "html_entity_decode",
                vec![
                    Value::from(numeric.as_str()),
                    Value::Int(doctype | 3),
                    Value::from(charset),
                ],
            );
        }
    }
    case("get_html_translation_table", vec![]);
    case("get_html_translation_table", vec![Value::Int(-7)]);
    case("html_entity_decode", vec![Value::from("&amp;&#9;")]);
    case(
        "html_entity_decode",
        vec![
            Value::from("no references"),
            Value::Int(3),
            Value::from("x-missing"),
        ],
    );

    for input in [
        Value::Null,
        Value::from(""),
        Value::Int(42),
        Value::from(format!("\u{26}#{}65;", "0".repeat(8192))),
    ] {
        case("html_entity_decode", vec![input]);
    }
    for wrong_type in [
        Value::from("invalid flag"),
        Value::from(phera_extension_api::PhpArray::new()),
    ] {
        case(
            "html_entity_decode",
            vec![Value::from("&amp;"), wrong_type.clone()],
        );
        case("get_html_translation_table", vec![wrong_type]);
    }
    case(
        "get_html_translation_table",
        vec![Value::Int(1), Value::Int(48), Value::Null],
    );
    case("html_entity_decode", vec![]);

    let Some(php) = oracle::run_php("web-basics-html-entities", &script) else {
        eprintln!("php binary not found; skipping");
        return;
    };
    oracle::compare("HTML entity functions", &labels, &php, &actual);
}
