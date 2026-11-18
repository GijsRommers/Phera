//! Request settings, padding, MIME and entity options crossed with scripts and encodings.
mod support;
use phera_extension_api::{PhpArray, Value};
use support::Matrix;
fn list(numbers: &[i64]) -> Value {
    let mut array = PhpArray::new();
    for n in numbers {
        array.push(Value::Int(*n)).unwrap();
    }
    Value::from(array)
}
#[test]
fn option_matrix() {
    let mut m = Matrix::new();
    for encoding in [
        "UTF-8",
        "ASCII",
        "ISO-8859-1",
        "Windows-1252",
        "UTF-16BE",
        "CP932",
    ] {
        for subject in [
            "",
            "aBc 123",
            "éßİΟΣ",
            "中文",
            "日本語",
            "العربية",
            "👩🏽‍💻e\u{301}",
        ] {
            let s = Value::from(subject);
            let e = Value::from(encoding);
            for length in [-2, 0, 1, 6, 30] {
                for kind in [-1, 0, 1, 2, 3] {
                    for padding in ["", " ", "éX"] {
                        m.case(
                            "mb_str_pad",
                            vec![
                                s.clone(),
                                Value::Int(length),
                                Value::from(padding),
                                Value::Int(kind),
                                e.clone(),
                            ],
                        );
                    }
                }
                for start in [-2, 0, 1, 40] {
                    for marker in ["", "…", "--"] {
                        m.case(
                            "mb_strimwidth",
                            vec![
                                s.clone(),
                                Value::Int(start),
                                Value::Int(length),
                                Value::from(marker),
                                e.clone(),
                            ],
                        );
                    }
                }
            }
            for map in [
                list(&[]),
                list(&[0]),
                list(&[0, 0x10ffff, 0, 0x7fffffff]),
                list(&[128, 65535, 100, 65535]),
            ] {
                for hex in [false, true] {
                    m.case(
                        "mb_encode_numericentity",
                        vec![s.clone(), map.clone(), e.clone(), Value::Bool(hex)],
                    );
                }
                m.case("mb_decode_numericentity", vec![s.clone(), map, e.clone()]);
            }
            for transfer in ["B", "Q", "", "other"] {
                for indent in [0, 12, 72] {
                    m.case(
                        "mb_encode_mimeheader",
                        vec![
                            s.clone(),
                            e.clone(),
                            Value::from(transfer),
                            Value::from("\r\n"),
                            Value::Int(indent),
                        ],
                    );
                }
            }
            for strict in [false, true] {
                for encodings in [
                    "auto",
                    "UTF-8,ASCII",
                    "ASCII,UTF-8,ISO-8859-1",
                    "UTF-16BE,CP932",
                ] {
                    m.case(
                        "mb_detect_encoding",
                        vec![s.clone(), Value::from(encodings), Value::Bool(strict)],
                    );
                }
            }
            for target in [
                "UTF-8",
                "ASCII",
                "ASCII//IGNORE",
                "ASCII//TRANSLIT",
                "ISO-8859-1",
                "UTF-16LE",
            ] {
                m.case("iconv", vec![e.clone(), Value::from(target), s.clone()]);
            }
            m.case("iconv_strlen", vec![s.clone(), e.clone()]);
            for start in [-30, -1, 0, 1, 30] {
                m.case(
                    "iconv_substr",
                    vec![s.clone(), Value::Int(start), Value::Null, e.clone()],
                );
                m.case(
                    "iconv_strpos",
                    vec![s.clone(), Value::from("a"), Value::Int(start), e.clone()],
                );
            }
            m.case(
                "iconv_strrpos",
                vec![s.clone(), Value::from("a"), e.clone()],
            );
        }
    }
    for input in [
        "plain",
        "=?UTF-8?B?w6k=?=",
        "=?UTF-8?Q?caf=C3=A9?=",
        "=?bogus?B?YQ==?=",
        "=?UTF-8?B?w6k=?= \r\n =?UTF-8?B?w6k=?=",
        "Subject: hello\r\nX-Test: a\r\nX-Test: b\r\n",
        // Header values are decoded as part of their line.
        "X:=?bogus?B?YQ==?=\r\n",
        "X:=?UTF-8?B?w6k=?=\r\nY:\t =?UTF-8?Q?a?= b\r\n",
        "=?UTF-8?B?WC1F?=: value\r\n",
        "X: a\r\n\t b\r\nY: =?UTF-8?B?w6k=?=",
        "=?UTF-8?B?w6k=?=",
        "=?UTF-8?B?w6k=?=\r\n",
        // Whitespace at the end, after words and around folds.
        "=?UTF-8?B?w6k=?= ",
        "=?UTF-8?B?w6k=?=\t ",
        "a ",
        "x =?UTF-8?B?w6k=?= y ",
        "=?UTF-8?B?w6k=?= \r\n z",
        "a  \r\n  b \r\n =?UTF-8?B?w6k=?= \r\n c\r\n ",
    ] {
        m.case("mb_decode_mimeheader", vec![Value::from(input)]);
        for mode in [0, 1, 2, 3, 4] {
            for encoding in ["UTF-8", "ASCII", "ISO-8859-1"] {
                for f in ["iconv_mime_decode", "iconv_mime_decode_headers"] {
                    m.case(
                        f,
                        vec![Value::from(input), Value::Int(mode), Value::from(encoding)],
                    );
                }
            }
        }
        m.case(
            "iconv_mime_encode",
            vec![Value::from("Subject"), Value::from(input)],
        );
    }
    // iconv folds encoded words between characters, never inside one.
    for value in [
        "",
        "a",
        &"Café crème brûlée, s'il vous plaît! ".repeat(4),
        &"日本語のテキスト".repeat(6),
    ] {
        // Stateful charsets are left out; the README says why.
        for charset in ["UTF-8", "ISO-8859-1", "UTF-16BE", "UTF-32LE"] {
            for scheme in ["B", "Q"] {
                for length in [20, 36, 76] {
                    let mut options = PhpArray::new();
                    for (key, option) in [
                        ("scheme", Value::from(scheme)),
                        ("output-charset", Value::from(charset)),
                        ("line-length", Value::Int(length)),
                    ] {
                        options.insert(phera_extension_api::ArrayKey::str(key), option);
                    }
                    m.case(
                        "iconv_mime_encode",
                        vec![
                            Value::from("Subject"),
                            Value::from(value),
                            Value::from(options),
                        ],
                    );
                }
            }
        }
    }
    // mb_decode_mimeheader reads plain text as ASCII: a byte from 0x80 is `?`.
    m.case(
        "mb_decode_mimeheader",
        vec![Value::from("caf\u{e9} =?UTF-8?B?w6k=?= x\u{e9}")],
    );
    // Encoded words are folded with `$newline` as given, bytes and all.
    for newline in [b"\n".to_vec(), b"\xFF\r\n".to_vec(), b"\r\n\t".to_vec()] {
        for scheme in ["B", "Q"] {
            m.case(
                "mb_encode_mimeheader",
                vec![
                    Value::from("é".repeat(40)),
                    Value::from("UTF-8"),
                    Value::from(scheme),
                    Value::from(newline.clone()),
                ],
            );
        }
    }
    m.compare("mbstring-options");
}
#[test]
fn settings_matrix() {
    let mut m = Matrix::new();
    for e in [
        "UTF-8",
        "utf8",
        "ASCII",
        "bad",
        "",
        "CP932",
        "UTF-16LE",
        "BASE64",
        "UUENCODE",
        "HTML-ENTITIES",
        "Quoted-Printable",
    ] {
        for f in [
            "mb_encoding_aliases",
            "mb_preferred_mime_name",
            "mb_internal_encoding",
            "mb_http_output",
            "mb_regex_encoding",
        ] {
            m.case(f, vec![Value::from(e)]);
        }
        for f in [
            "mb_internal_encoding",
            "mb_http_output",
            "mb_regex_encoding",
        ] {
            m.case(f, vec![]);
        }
        for f in ["mb_get_info", "iconv_get_encoding"] {
            m.case(f, vec![]);
        }
    }
    m.case("mb_internal_encoding", vec![Value::from("UTF-8")]);
    for value in [
        Value::Null,
        Value::Int(0),
        Value::Int(63),
        Value::Int(0xfffd),
        Value::Int(0x110000),
        Value::from("none"),
        Value::from("long"),
        Value::from("entity"),
        Value::from("bad"),
    ] {
        m.case("mb_substitute_character", vec![value]);
        m.case("mb_substitute_character", vec![]);
        for s in [vec![0xff], vec![0xe2, 0x82], "😀".as_bytes().to_vec()] {
            for target in ["UTF-8", "ASCII", "UTF-16LE", "CP932"] {
                m.case(
                    "mb_convert_encoding",
                    vec![
                        Value::from(s.clone()),
                        Value::from(target),
                        Value::from("UTF-8"),
                    ],
                );
            }
        }
    }
    m.compare("mbstring-settings");
}

/// `mb_language()` decides the mail defaults that `mb_get_info()` reports
/// and the charset `mb_encode_mimeheader` uses when given none.
#[test]
fn mail_settings_follow_the_language() {
    let mut m = Matrix::new();
    for language in [
        "neutral",
        "uni",
        "Japanese",
        "en",
        "German",
        "Russian",
        "Korean",
        "Armenian",
        "Turkish",
        "ua",
        "Simplified Chinese",
        "zh-tw",
    ] {
        m.case("mb_language", vec![Value::from(language)]);
        m.case("mb_language", vec![]);
        for key in ["mail_charset", "mail_header_encoding", "mail_body_encoding"] {
            m.case("mb_get_info", vec![Value::from(key)]);
        }
        // Without a charset, the language's mail charset is used. Phera
        // lacks those of Korean, Armenian and Simplified Chinese.
        if !["Korean", "Armenian", "Simplified Chinese"].contains(&language) {
            m.case("mb_encode_mimeheader", vec![Value::from("Café")]);
        }
    }
    m.compare("mbstring-mail-settings");
}
