//! Compare Japanese width conversion: `mb_convert_kana('ｶﾞ', 'KV') === 'ガ'`.
mod support;
use phera_extension_api::Value;
use support::Matrix;
#[test]
fn kana_options() {
    let mut matrix = Matrix::new();
    let alphabet = "ARNSKHMCarnskhmcV";
    let mut modes = vec![
        String::new(),
        "KV".into(),
        "HV".into(),
        "bad".into(),
        "\0".into(),
    ];
    for first in alphabet.chars() {
        modes.push(first.to_string());
        for second in alphabet.chars() {
            modes.push(format!("{first}{second}"));
        }
    }
    let subjects = [
        "",
        "ABC abc 123 !~\\'\"",
        "ＺＡ１２　ｱｲｳｴｵｶﾞﾊﾟ",
        "あいうえおガパヴゔヷヺー。、",
        "é Ελληνικά 中文 العربية 👩🏽‍💻e\u{301}",
    ];
    for encoding in [
        "UTF-8",
        "ASCII",
        "UTF-16BE",
        "ISO-8859-1",
        "CP932",
        "EUC-JP",
    ] {
        for subject in subjects {
            for mode in &modes {
                matrix.case(
                    "mb_convert_kana",
                    vec![
                        Value::from(subject),
                        Value::from(mode.clone()),
                        Value::from(encoding),
                    ],
                );
            }
        }
    }
    matrix.compare("mbstring-kana");
}
