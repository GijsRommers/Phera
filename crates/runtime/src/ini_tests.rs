//! One worker can run `ini_set('default_charset', 'ASCII')` in one request
//! and start the next request with UTF-8.

use std::sync::Arc;

use phera_ext_http::Sapi;
use phera_extension_api::testing::oracle;

async fn run(runtime: &super::Runtime, source: &str) -> String {
    let unit = phera_compiler::compile(source, "ini-probe.php").unwrap();
    let mut vm = runtime.new_vm(Sapi::Cli);
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    String::from_utf8(vm.output).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn requests_on_one_worker_keep_their_own_ini_changes() {
    let runtime = super::Runtime::new();
    let source =
        "<?php echo ini_get('default_charset'), '|', get_include_path(), '|', error_reporting();";
    let change = r#"<?php
ini_set('default_charset', 'ASCII');
set_include_path('/tmp');
error_reporting(1);
echo ini_get('default_charset'), '|', get_include_path(), '|', error_reporting();
"#;
    assert_eq!(run(&runtime, change).await, "ASCII|/tmp|1");
    let php = oracle::find_php().expect("PHP oracle");
    let pristine = std::process::Command::new(php)
        .args([
            "-n",
            "-r",
            "echo ini_get('default_charset'), '|', error_reporting();",
        ])
        .output()
        .unwrap();
    assert!(pristine.status.success());
    let start = run(&runtime, source).await;
    assert_eq!(start.replace("|.|", "|").as_bytes(), pristine.stdout);
    assert_eq!(run(&runtime, source).await, start);
}

#[tokio::test(flavor = "current_thread")]
async fn quantity_conversions_match_php() {
    let mut source = String::from("<?php\n");
    source.push_str(oracle::PHP_PREAMBLE);
    let numbers = [
        "",
        "0",
        "007",
        "08",
        "0x",
        "0x21",
        "0b11",
        "0o4",
        "0b2",
        "0oo",
        "1.5",
        "abc",
        "+",
        "-0x-4",
        "4",
        "4294967296",
        "9223372036854775807",
        "-9223372036854775808",
        "-9223372036854775809",
        "18446744073709551616",
    ];
    let suffixes = [
        "",
        "k",
        "M",
        "g",
        " K ",
        "terabytes",
        "MB",
        " bytesG",
        "\t",
        "\0",
        "\u{80}",
    ];
    let mut case = 0;
    for number in numbers {
        for suffix in suffixes {
            let bytes = format!("{number}{suffix}");
            let hex: String = bytes
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            source.push_str(&format!(
                "run('{case}', fn() => ini_parse_quantity(hex2bin('{hex}')));\n"
            ));
            case += 1;
        }
    }
    let Some(expected) = oracle::run_php("ini-quantities", &source) else {
        return;
    };
    let actual = run(&super::Runtime::new(), &source).await;
    assert_eq!(expected.len(), case);
    let actual: Vec<_> = actual.lines().collect();
    assert_eq!(actual.len(), case);
    let actual = actual.into_iter().map(str::to_owned).collect::<Vec<_>>();
    oracle::compare("INI quantity conversions", &[], &expected, &actual);
}
