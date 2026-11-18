//! Transfer encodings preserve bytes: `mb_convert_encoding('abc', 'BASE64') === 'YWJj'`.
mod support;
use phera_extension_api::Value;
use support::Matrix;
#[test]
fn transfer_matrix() {
    let mut matrix = Matrix::new();
    let subjects = [
        Vec::new(),
        b"ASCII text\r\nwith trailing space \t\r\n".to_vec(),
        "é Ελληνικά 中文 日本語 العربية 👩🏽‍💻e\u{301}"
            .as_bytes()
            .to_vec(),
        vec![0xff, 0xc0, 0xaf, 0xed, 0xa0, 0x80, 0xe2, 0x82],
        b"YWJj=?=abc=&amp;&#128512;&#xD800;".to_vec(),
        b"begin 0644 filename\n#86)C\n`\nend\n".to_vec(),
        vec![b'A'; 1000],
    ];
    for transfer in [
        "BASE64",
        "UUENCODE",
        "HTML-ENTITIES",
        "Quoted-Printable",
        "7bit",
        "8bit",
    ] {
        for encoding in [
            "UTF-8",
            "ASCII",
            "UTF-16BE",
            "ISO-8859-1",
            "CP932",
            "EUC-JP",
        ] {
            for subject in &subjects {
                for (to, from) in [(transfer, encoding), (encoding, transfer)] {
                    matrix.case(
                        "mb_convert_encoding",
                        vec![
                            Value::from(subject.clone()),
                            Value::from(to),
                            Value::from(from),
                        ],
                    );
                }
                for function in [
                    "mb_strlen",
                    "mb_check_encoding",
                    "mb_scrub",
                    "mb_strtoupper",
                ] {
                    matrix.case(
                        function,
                        vec![Value::from(subject.clone()), Value::from(transfer)],
                    );
                }
            }
        }
    }
    matrix.compare("mbstring-transfer");
}

/// PHP deprecates a transfer encoding once per new name. Functions built on
/// others (`mb_ucfirst` on `mb_substr`, `mb_str_pad` on `mb_strlen`) must not
/// raise it again under the inner function's name, nor raise it at all when
/// the encoding comes from `mb_internal_encoding()`.
#[test]
fn transfer_deprecations_come_once_from_the_called_function() {
    let mut matrix = Matrix::new();
    let subject = || Value::from(" YWJj=3D ");
    for name in ["qprint", "html", "HTML", "Quoted-Printable", "BASE64"] {
        let encoding = || Value::from(name);
        for function in ["mb_ucfirst", "mb_lcfirst"] {
            matrix.case(function, vec![subject(), encoding()]);
        }
        for function in ["mb_strstr", "mb_stristr", "mb_strrchr", "mb_strrichr"] {
            for before in [false, true] {
                matrix.case(
                    function,
                    vec![subject(), Value::from("J"), Value::Bool(before), encoding()],
                );
            }
        }
        for function in ["mb_trim", "mb_ltrim", "mb_rtrim"] {
            matrix.case(function, vec![subject(), Value::Null, encoding()]);
        }
        matrix.case(
            "mb_str_pad",
            vec![
                subject(),
                Value::Int(12),
                Value::from("-"),
                Value::Int(2),
                encoding(),
            ],
        );
        // The same name again, which PHP does not deprecate a second time.
        matrix.case("mb_strlen", vec![subject(), encoding()]);
    }
    for internal in ["BASE64", "HTML-ENTITIES", "UTF-8"] {
        matrix.case("mb_internal_encoding", vec![Value::from(internal)]);
        for function in ["mb_ucfirst", "mb_lcfirst", "mb_trim"] {
            matrix.case(function, vec![subject()]);
        }
        matrix.case("mb_strstr", vec![subject(), Value::from("J")]);
        matrix.case("mb_str_pad", vec![subject(), Value::Int(12)]);
    }
    matrix.compare("mbstring-transfer-deprecations");
}
