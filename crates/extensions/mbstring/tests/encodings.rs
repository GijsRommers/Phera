//! Round-trip every representable scalar through every advertised character encoding.
mod support;

use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
#[test]
fn every_representable_scalar() {
    let mut registry = Registry::new();
    Mbstring.register(&mut registry);
    let mut ctx = TestContext::default();
    let mut comparisons = 0;
    let mut mismatches = 0;
    let mut examples = Vec::new();
    let Ok(Value::Array(listed)) = call(&registry, &mut ctx, "mb_list_encodings", vec![]) else {
        panic!("mb_list_encodings returns an array");
    };
    let transfer = ["BASE64", "UUENCODE", "HTML-ENTITIES", "Quoted-Printable"];
    let names: Vec<String> = listed
        .iter()
        .filter_map(|(_, name)| match name {
            Value::String(name) => Some(name.to_string_lossy()),
            _ => None,
        })
        .filter(|name| !transfer.contains(&name.as_str()))
        .collect();
    assert_eq!(names.len(), 44, "character encodings");
    for name in &names {
        let name = name.as_str();
        let script = format!(
            r#"<?php
for ($cp = 0; $cp <= 0x10FFFF; $cp++) {{
    if ('{name}' === 'ISO-2022-JP') {{
        mb_substitute_character('none');
        $u = mb_chr($cp, 'UTF-8');
        $s = $u === false ? false : mb_convert_encoding($u, '{name}', 'UTF-8');
        if ($s === '') $s = false;
    }} else {{ $s = mb_chr($cp, '{name}'); }}
    if ($s !== false) echo $cp, '|', bin2hex($s), '|',
        bin2hex(mb_convert_encoding($s, 'UTF-8', '{name}')), "\n";
}}
"#
        );
        let lines = oracle::run_php(&format!("mbstring-roundtrip-{name}"), &script).unwrap();
        let count = lines.len();
        for line in lines {
            let mut fields = line.split('|');
            let cp: i64 = fields.next().unwrap().parse().unwrap();
            let expected = fields.next().unwrap();
            let back = fields.next().unwrap();
            let encoded = if name == "ISO-2022-JP" {
                call(
                    &registry,
                    &mut ctx,
                    "mb_substitute_character",
                    vec![Value::from("none")],
                )
                .unwrap();
                let utf8 = char::from_u32(cp as u32).unwrap().to_string();
                call(
                    &registry,
                    &mut ctx,
                    "mb_convert_encoding",
                    vec![Value::from(utf8), Value::from(name), Value::from("UTF-8")],
                )
            } else {
                call(
                    &registry,
                    &mut ctx,
                    "mb_substitute_character",
                    vec![Value::Int(63)],
                )
                .unwrap();
                call(
                    &registry,
                    &mut ctx,
                    "mb_chr",
                    vec![Value::Int(cp), Value::from(name)],
                )
            };
            let decoded = encoded.as_ref().ok().and_then(|v| {
                call(
                    &registry,
                    &mut ctx,
                    "mb_convert_encoding",
                    vec![v.clone(), Value::from("UTF-8"), Value::from(name)],
                )
                .ok()
            });
            comparisons += 2;
            let actual = encoded
                .as_ref()
                .map(oracle::encode)
                .unwrap_or_else(|e| format!("{e:?}"));
            let decoded = decoded.as_ref().map(oracle::encode).unwrap_or_default();
            if actual != format!("S:{expected}") || decoded != format!("S:{back}") {
                mismatches += 1;
                if examples.len() < 30 {
                    examples.push(format!(
                        "{name} U+{cp:04X}: PHP {expected}/{back}, Rust {actual}/{decoded}"
                    ));
                }
            }
        }
        println!("{name}: {count} representable scalars");
    }
    assert_eq!(
        mismatches,
        0,
        "{comparisons} calls, {mismatches} mismatched round trips\n{}",
        examples.join("\n")
    );
    println!("Encoding round trips: {comparisons} calls, zero mismatches");
}

/// PHP 8.5 lists 79 encodings and Phera 48. Every encoding Phera lists must
/// be in PHP's list, in the same order, with the same aliases and MIME name,
/// and each alias must name the same encoding in PHP.
#[test]
fn listed_encodings_exist_in_php_with_the_same_names() {
    let mut matrix = support::Matrix::new();
    let Ok(Value::Array(listed)) = call(
        &matrix.registry,
        &mut matrix.context,
        "mb_list_encodings",
        vec![],
    ) else {
        panic!("mb_list_encodings returns an array");
    };
    let names: Vec<Value> = listed.iter().map(|(_, name)| name.clone()).collect();
    matrix.case("mb_list_encodings", vec![]);
    for name in &names {
        matrix.case("mb_encoding_aliases", vec![name.clone()]);
        matrix.case("mb_preferred_mime_name", vec![name.clone()]);
        let Ok(Value::Array(aliases)) = call(
            &matrix.registry,
            &mut matrix.context,
            "mb_encoding_aliases",
            vec![name.clone()],
        ) else {
            panic!("mb_encoding_aliases returns an array");
        };
        for (_, alias) in aliases.iter() {
            // An alias names the same encoding: the same canonical aliases.
            matrix.case("mb_encoding_aliases", vec![alias.clone()]);
        }
    }
    matrix.compare("mbstring-encoding-names");
}
