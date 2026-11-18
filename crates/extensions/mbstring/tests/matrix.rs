//! Scripts, byte errors and boundary offsets crossed with the encoding options.
mod support;
use phera_extension_api::Value;
use support::Matrix;
#[test]
fn text_matrix() {
    let subjects: Vec<Vec<u8>> = [
        "",
        "ASCII",
        "éßİΣΟΣ",
        "中文日本語",
        "한글",
        "العربية",
        "עברית",
        "Кириллица",
        "हिन्दी",
        "e\u{301}",
        "👩🏽‍💻🏳️‍🌈",
        " \t\0a\u{3000}",
        "a\0b",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .chain([
        vec![0xff],
        vec![0xe2, 0x82],
        vec![0xed, 0xa0, 0x80],
        vec![0xc0, 0xaf],
        vec![0xf4, 0x90, 0x80, 0x80],
        vec![0xe2, b'A'],
    ])
    .collect();
    let mut m = Matrix::new();
    for enc in [
        "UTF-8",
        "ASCII",
        "ISO-8859-1",
        "Windows-1252",
        "UTF-16BE",
        "UTF-32LE",
        "SJIS",
        "CP932",
        "EUC-JP",
        "GB18030",
        "EUC-KR",
        "BIG-5",
    ] {
        for s in &subjects {
            let s = Value::from(s.clone());
            let e = Value::from(enc);
            for f in [
                "mb_strlen",
                "mb_strtoupper",
                "mb_strtolower",
                "mb_ucfirst",
                "mb_lcfirst",
                "mb_strwidth",
                "mb_ord",
                "mb_scrub",
                "mb_check_encoding",
            ] {
                m.case(f, vec![s.clone(), e.clone()]);
            }
            for mode in -1..=8 {
                m.case(
                    "mb_convert_case",
                    vec![s.clone(), Value::Int(mode), e.clone()],
                );
            }
            for offset in [-30, -2, -1, 0, 1, 2, 30, i64::MAX, i64::MIN] {
                for length in [
                    Value::Null,
                    Value::Int(-2),
                    Value::Int(0),
                    Value::Int(1),
                    Value::Int(30),
                ] {
                    for f in ["mb_substr", "mb_strcut"] {
                        m.case(
                            f,
                            vec![s.clone(), Value::Int(offset), length.clone(), e.clone()],
                        );
                    }
                }
                for needle in ["", "a", "Σ", "é"] {
                    for f in ["mb_strpos", "mb_strrpos", "mb_stripos", "mb_strripos"] {
                        m.case(
                            f,
                            vec![
                                s.clone(),
                                Value::from(needle),
                                Value::Int(offset),
                                e.clone(),
                            ],
                        );
                    }
                }
            }
            for length in [-1, 0, 1, 2, 10] {
                m.case(
                    "mb_str_split",
                    vec![s.clone(), Value::Int(length), e.clone()],
                );
            }
            for needle in ["", "a", "é", "Σ"] {
                m.case(
                    "mb_substr_count",
                    vec![s.clone(), Value::from(needle), e.clone()],
                );
                for before in [false, true] {
                    for f in ["mb_strstr", "mb_stristr", "mb_strrchr", "mb_strrichr"] {
                        m.case(
                            f,
                            vec![
                                s.clone(),
                                Value::from(needle),
                                Value::Bool(before),
                                e.clone(),
                            ],
                        );
                    }
                }
            }
            for f in ["mb_trim", "mb_ltrim", "mb_rtrim"] {
                for set in [Value::Null, Value::from(""), Value::from(" aé")] {
                    m.case(f, vec![s.clone(), set, e.clone()]);
                }
            }
            for target in [
                "UTF-8",
                "ASCII",
                "UTF-16BE",
                "ISO-8859-1",
                "Windows-1252",
                "CP932",
            ] {
                m.case(
                    "mb_convert_encoding",
                    vec![s.clone(), Value::from(target), e.clone()],
                );
            }
        }
    }
    m.compare("mbstring-text-matrix");
}
#[test]
fn hostile_arguments() {
    let mut m = Matrix::new();
    let functions: Vec<_> = m.registry.functions().map(|e| e.name).collect();
    for f in functions {
        for value in [
            Value::Null,
            Value::Bool(false),
            Value::Int(4),
            Value::Float(1.5),
            Value::from(""),
            Value::from("unknown"),
            Value::from(phera_extension_api::PhpArray::new()),
        ] {
            for count in [0, 1, 6] {
                m.case(f, vec![value.clone(); count]);
            }
        }
    }
    m.compare("mbstring-hostile");
}
