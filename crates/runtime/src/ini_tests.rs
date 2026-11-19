//! One worker can run `ini_set('default_charset', 'ASCII')` in one request
//! and start the next request with UTF-8.

use std::sync::Arc;

use phera_ext_http::Sapi;
use phera_extension_api::testing::oracle;
use phera_vm::NativeTables;

use crate::{HttpRequest, Runtime};

async fn run(runtime: &super::Runtime, tables: &NativeTables, source: &str) -> String {
    let unit = phera_compiler::compile(source, "ini-probe.php").unwrap();
    let mut vm = runtime.new_vm(tables, Sapi::Cli, runtime.inherited_environment());
    assert_eq!(vm.run(Arc::new(unit)).await, 0);
    String::from_utf8(vm.output).unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn requests_on_one_worker_keep_their_own_ini_changes() {
    let runtime = super::Runtime::new();
    let tables = NativeTables::new(runtime.registry.clone());
    let source =
        "<?php echo ini_get('default_charset'), '|', get_include_path(), '|', error_reporting();";
    let change = r#"<?php
ini_set('default_charset', 'ASCII');
set_include_path('/tmp');
error_reporting(1);
echo ini_get('default_charset'), '|', get_include_path(), '|', error_reporting();
"#;
    assert_eq!(run(&runtime, &tables, change).await, "ASCII|/tmp|1");
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
    let start = run(&runtime, &tables, source).await;
    assert_eq!(start.replace("|.|", "|").as_bytes(), pristine.stdout);
    assert_eq!(run(&runtime, &tables, source).await, start);
}

#[tokio::test(flavor = "current_thread")]
async fn compiled_native_calls_use_the_request_inventory_and_ini_values() {
    let runtime = Runtime::new();
    let directory = tempfile::tempdir().unwrap();
    let entry = directory.path().join("index.php");
    std::fs::write(&entry, "<?php require __DIR__ . '/probe.php';").unwrap();
    std::fs::write(
        directory.path().join("probe.php"),
        r#"<?php
if (isset($_GET['change'])) {
    ini_set('default_charset', 'ASCII');
    define('REQUEST_VALUE', 'present');
}

echo constant('PHP_SAPI') === php_sapi_name() ? 'sapi' : 'wrong';
echo '|', (int) function_exists('getallheaders'), '|', ini_get('default_charset'),
    '|', strlen("é"), '|', (int) defined('REQUEST_VALUE');
"#,
    )
    .unwrap();

    let cli = runtime.run_file(&entry, &[], None).await.unwrap();
    assert_eq!(cli.status, 0, "{}", String::from_utf8_lossy(&cli.output));
    assert_eq!(cli.output, b"sapi|0|UTF-8|2|0");
    let worker = runtime.web_worker();
    for (query, expected) in [
        ("change=1", b"sapi|1|ASCII|2|1".as_slice()),
        ("", b"sapi|1|UTF-8|2|0".as_slice()),
    ] {
        let response = worker
            .handle_request(
                &entry,
                directory.path(),
                HttpRequest {
                    method: "GET".into(),
                    target: format!("/index.php?{query}"),
                    path: "/index.php".into(),
                    query: query.into(),
                    protocol: "HTTP/1.1".into(),
                    headers: Vec::new(),
                    body: Vec::new(),
                    remote_addr: ([127, 0, 0, 1], 8081).into(),
                    local_addr: ([127, 0, 0, 1], 8080).into(),
                    https: false,
                },
            )
            .await;
        assert_eq!(response.status, 200);
        assert_eq!(response.body, expected);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unavailable_builtin_names_keep_user_function_binding() {
    let source = "<?php function strlen($value) { return 90; } echo strlen('bytes');";
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("disabled.php");
    std::fs::write(&file, source).unwrap();
    let php = oracle::find_php().expect("PHP oracle");
    let expected = std::process::Command::new(php)
        .args(["-n", "-d", "disable_functions=strlen"])
        .arg(&file)
        .output()
        .unwrap();
    assert!(expected.status.success());

    let registry = Arc::new(phera_extension_api::Registry::new());
    let unit = crate::cache::CodeCache::new(registry.clone())
        .compile(file)
        .await
        .unwrap();
    let mut vm = phera_vm::Vm::new(registry);
    assert_eq!(vm.run(unit).await, 0);
    assert_eq!(vm.output, expected.stdout);
}

#[tokio::test(flavor = "current_thread")]
async fn shared_native_classes_use_request_local_method_names() {
    let runtime = Runtime::new();
    let tables = NativeTables::new(runtime.registry.clone());
    let php = oracle::find_php().expect("PHP oracle");
    for snippet in [
        "$r = new ReflectionClass(stdClass::class); var_dump($r->getName());",
        "$r = new ReflectionClass(stdClass::class); var_dump($r->isInternal());",
        "$r = new ReflectionClass(stdClass::class); var_dump($r->getName());",
    ] {
        let expected = std::process::Command::new(&php)
            .args(["-n", "-r", snippet])
            .output()
            .unwrap();
        assert!(expected.status.success());
        let source = format!("<?php {snippet}");
        assert_eq!(
            run(&runtime, &tables, &source).await.as_bytes(),
            expected.stdout
        );
    }
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
    let runtime = super::Runtime::new();
    let tables = NativeTables::new(runtime.registry.clone());
    let actual = run(&runtime, &tables, &source).await;
    assert_eq!(expected.len(), case);
    let actual: Vec<_> = actual.lines().collect();
    assert_eq!(actual.len(), case);
    let actual = actual.into_iter().map(str::to_owned).collect::<Vec<_>>();
    oracle::compare("INI quantity conversions", &[], &expected, &actual);
}
