//! One-megabyte subjects test real results independently of the timing harness.
mod support;

use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
#[test]
fn megabyte_strings() {
    let mut registry = Registry::for_level(phera_extension_api::testing::oracle::test_level());
    Mbstring.register(&mut registry);
    let pattern = "AéΣ中👩🏽‍💻e\u{301}";
    let repeat = 1_048_576 / pattern.len();
    let value = Value::from(pattern.repeat(repeat));
    for encoding in [
        "UTF-8",
        "ASCII",
        "ISO-8859-1",
        "Windows-1252",
        "UTF-16BE",
        "CP932",
    ] {
        let mut ctx = TestContext::default();
        let mut script = format!(
            "<?php\n{}\n$s=str_repeat({}, {repeat});\n",
            oracle::PHP_PREAMBLE,
            oracle::php_literal(&Value::from(pattern))
        );
        let mut rust = Vec::new();
        for function in [
            "mb_strlen",
            "mb_strtoupper",
            "mb_strtolower",
            "mb_strwidth",
            "mb_check_encoding",
            "mb_scrub",
            "mb_ucfirst",
            "mb_lcfirst",
        ] {
            let result = call(
                &registry,
                &mut ctx,
                function,
                vec![value.clone(), Value::from(encoding)],
            );
            script.push_str(&format!(
                "run('large', fn()=>{function}($s,'{encoding}'));\n"
            ));
            rust.push(oracle::encode_case(
                "large",
                &result,
                &ctx.output,
                &ctx.diagnostics,
            ));
        }
        for mode in 0..8 {
            let result = call(
                &registry,
                &mut ctx,
                "mb_convert_case",
                vec![value.clone(), Value::Int(mode), Value::from(encoding)],
            );
            script.push_str(&format!(
                "run('large', fn()=>mb_convert_case($s,{mode},'{encoding}'));\n"
            ));
            rust.push(oracle::encode_case(
                "large",
                &result,
                &ctx.output,
                &ctx.diagnostics,
            ));
        }
        for start in [-400_000, 0, 123] {
            let result = call(
                &registry,
                &mut ctx,
                "mb_substr",
                vec![
                    value.clone(),
                    Value::Int(start),
                    Value::Int(400_000),
                    Value::from(encoding),
                ],
            );
            script.push_str(&format!(
                "run('large', fn()=>mb_substr($s,{start},400000,'{encoding}'));\n"
            ));
            rust.push(oracle::encode_case(
                "large",
                &result,
                &ctx.output,
                &ctx.diagnostics,
            ));
        }
        let php = oracle::run_php(&format!("mbstring-large-{encoding}"), &script).unwrap();
        oracle::compare(&format!("megabyte {encoding}"), &[], &php, &rust);
    }
}

/// What `iconv(..., 'ISO-8859-1//IGNORE', $utf8)` must give: the valid
/// UTF-8 decoded, the characters above U+00FF left out, the rest as bytes.
fn latin1_ignoring(utf8: &[u8]) -> Vec<u8> {
    utf8.utf8_chunks()
        .flat_map(|chunk| chunk.valid().chars())
        .filter_map(|character| u8::try_from(u32::from(character)).ok())
        .collect()
}

/// A megabyte through every iconv function, which from 64 KiB converts on
/// Tokio's blocking threads. Results and diagnostics must be PHP's; for
/// //IGNORE on glibc, where PHP's own result has gaps, Phera's bytes are
/// checked against `latin1_ignoring`.
#[test]
fn megabyte_iconv() {
    let mixed = "AéΣ中👩🏽‍💻e\u{301}".repeat(40_000);
    let latin = "Abcé déjà vu, naïve façade. ".repeat(37_000);
    let mut broken = mixed.clone().into_bytes();
    for index in (1000..broken.len()).step_by(4099) {
        broken[index] = 0xFF;
    }
    let mut incomplete = "é".repeat(40_000).into_bytes();
    incomplete.push(0xC3);
    let mut illegal = "é".repeat(40_000).into_bytes();
    illegal.push(0xFF);
    let words = "=?UTF-8?B?w6k=?= =?UTF-8?Q?caf=C3=A9?= ".repeat(25_000);
    let mut matrix = support::Matrix::new();
    let utf8 = || Value::from("UTF-8");
    let mut ignoring = Vec::new();
    for subject in [
        &mixed.as_bytes().to_vec(),
        &latin.as_bytes().to_vec(),
        &broken,
    ] {
        let subject = Value::from(subject.clone());
        for to in [
            "UTF-16LE",
            "ISO-8859-1",
            "ISO-8859-1//IGNORE",
            "ASCII//TRANSLIT",
        ] {
            let arguments = vec![utf8(), Value::from(to), subject.clone()];
            if to.ends_with("//IGNORE") {
                ignoring.push(arguments.clone());
            }
            matrix.case("iconv", arguments);
        }
        matrix.case("iconv_strlen", vec![subject.clone()]);
        matrix.case(
            "iconv_substr",
            vec![subject.clone(), Value::Int(1000), Value::Int(100_000)],
        );
        matrix.case("iconv_substr", vec![subject.clone(), Value::Int(-5000)]);
        for needle in ["中👩", "vu, n", "\u{301}A"] {
            matrix.case(
                "iconv_strpos",
                vec![subject.clone(), Value::from(needle), Value::Int(300_000)],
            );
            matrix.case(
                "iconv_strpos",
                vec![subject.clone(), Value::from(needle), Value::Int(-1000)],
            );
            matrix.case("iconv_strrpos", vec![subject.clone(), Value::from(needle)]);
        }
    }
    for subject in [&incomplete, &illegal] {
        for to in ["UTF-16BE", "ISO-8859-1//IGNORE"] {
            matrix.case(
                "iconv",
                vec![utf8(), Value::from(to), Value::from(subject.clone())],
            );
        }
        matrix.case("iconv_strlen", vec![Value::from(subject.clone())]);
    }
    matrix.case(
        "iconv_mime_encode",
        vec![Value::from("Subject"), Value::from(latin.clone())],
    );
    matrix.case("iconv_mime_decode", vec![Value::from(words.clone())]);
    matrix.case(
        "iconv_mime_decode",
        vec![
            Value::from(words.clone()),
            Value::Int(2),
            Value::from("ISO-8859-1//IGNORE"),
        ],
    );
    matrix.case(
        "iconv_mime_decode_headers",
        vec![Value::from(format!("Subject: {words}\r\nX: y\r\n"))],
    );
    for arguments in ignoring {
        let Some(Value::String(subject)) = arguments.get(2) else {
            unreachable!("the subject is a string");
        };
        let expected = latin1_ignoring(subject.as_bytes());
        let result = matrix.call("iconv", arguments.clone());
        assert!(
            matches!(&result, Ok(Value::String(converted)) if converted.as_bytes() == expected),
            "iconv to ISO-8859-1//IGNORE of {} bytes gives the clean conversion",
            subject.len()
        );
    }
    matrix.compare("mbstring-large-iconv");
}
