//! One-megabyte subjects test real results independently of the timing harness.
mod support;

use phera_ext_mbstring::Mbstring;
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Extension, Registry, Value};
#[test]
fn megabyte_strings() {
    let mut registry = Registry::new();
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

/// From 64 KiB on, iconv converts on Tokio's blocking threads; the results
/// and diagnostics must stay PHP's.
#[test]
fn megabyte_iconv() {
    let text = Value::from("AéΣ中👩🏽‍💻e\u{301}".repeat(30_000));
    let mut invalid = "é".repeat(40_000).into_bytes();
    invalid.push(0xFF);
    let mut matrix = support::Matrix::new();
    for (from, to) in [
        ("UTF-8", "UTF-16LE"),
        ("UTF-8", "ISO-8859-1//IGNORE"),
        ("UTF-8", "ASCII"),
    ] {
        matrix.case(
            "iconv",
            vec![Value::from(from), Value::from(to), text.clone()],
        );
    }
    matrix.case(
        "iconv",
        vec![
            Value::from("UTF-8"),
            Value::from("UTF-16BE"),
            Value::from(invalid.clone()),
        ],
    );
    matrix.case("iconv_strlen", vec![text.clone()]);
    matrix.case("iconv_strlen", vec![Value::from(invalid)]);
    matrix.case(
        "iconv_substr",
        vec![text, Value::Int(1000), Value::Int(100_000)],
    );
    matrix.compare("mbstring-large-iconv");
}
