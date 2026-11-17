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
