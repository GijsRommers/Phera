//! Runs the same curl scripts in Phera and real PHP 8.5.

use phera_extension_api::testing::oracle;
use phera_runtime::Runtime;

mod support;

fn compare(name: &str, script: &str) {
    let php = oracle::run_php(name, script).expect("PHP 8.5 oracle must be installed");
    let path = oracle::script_path(name);
    let result = Runtime::new()
        .run_file_blocking(path, &[], None)
        .expect("run Phera");
    let output = String::from_utf8(result.output).unwrap();
    assert_eq!(result.status, 0, "{output}");
    let rust: Vec<_> = output.lines().map(str::to_owned).collect();
    let labels: Vec<_> = php
        .iter()
        .map(|line| line.split('=').next().unwrap().to_owned())
        .collect();
    oracle::compare(name, &labels, &php, &rust);
}

#[test]
fn native_api() {
    let script = format!(
        "<?php\n{}\n{}",
        oracle::PHP_PREAMBLE,
        include_str!("api.php")
    );
    compare("curl-native-api", &script);
}

#[test]
fn local_http() {
    let directory = tempfile::tempdir().unwrap();
    let server = support::Server::start(directory.path());
    let ca = oracle::php_literal(&phera_extension_api::Value::from(
        server.ca.to_string_lossy().into_owned(),
    ));
    let tmp = oracle::php_literal(&phera_extension_api::Value::from(
        directory.path().to_string_lossy().into_owned(),
    ));
    let script = format!(
        "<?php\n{}\n$base = 'http://{}';\n$tls = 'https://localhost:{}';\n$ca = {ca};\n$tmp = {tmp};\n{}",
        oracle::PHP_PREAMBLE,
        server.address,
        server.tls_address.port(),
        include_str!("local_http.php")
    );
    compare("curl-local-http", &script);
}
