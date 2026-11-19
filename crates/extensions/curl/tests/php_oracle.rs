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
    let fields = oracle::run_php(
        "curl-library-fields",
        "<?php echo implode(',', array_keys(curl_getinfo(curl_init())));",
    )
    .expect("PHP 8.5 oracle must be installed");
    let keys = fields.first().expect("curl_getinfo keys");
    let script = format!(
        "<?php\n{}\n$oracleFields = explode(',', '{keys}');\n{}",
        oracle::PHP_PREAMBLE,
        include_str!("api.php")
    );
    compare("curl-native-api", &script);
}

#[test]
fn constant_catalog() {
    use phera_extension_api::{ConstantValue, Registry};

    let php = oracle::run_php(
        "curl-library-constants",
        concat!(
            "<?php foreach (get_defined_constants(true)['curl'] as $name => $value) ",
            "echo $name, ' ', $value, PHP_EOL;",
        ),
    )
    .expect("PHP 8.5 oracle must be installed");
    let mut registry = Registry::new();
    registry.load(&phera_ext_curl::Curl);
    assert_eq!(registry.constants().count(), 689);
    for line in &php {
        let (name, number) = line.split_once(' ').expect("cURL constant record");
        let expected = ConstantValue::Int(number.parse::<i64>().expect("cURL constant number"));
        assert_eq!(registry.get_constant(name), Some(&expected), "{name}");
    }
    println!(
        "curl constant catalog: {} PHP constants, zero mismatches",
        php.len()
    );
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
    let unix = oracle::php_literal(&phera_extension_api::Value::from(
        server.unix.to_string_lossy().into_owned(),
    ));
    let prefix = format!(
        "<?php\n{}\n$base = 'http://{}';\n$tls = 'https://localhost:{}';\n",
        oracle::PHP_PREAMBLE,
        server.address,
        server.tls_address.port()
    );
    let setup = format!("$ca = {ca};\n$tmp = {tmp};\n$unix = {unix};\n");
    let extra = if oracle::supports_php_since("curl copy stream ownership", 80_513) {
        include_str!("copy_trace.php")
    } else {
        ""
    };
    let script = format!("{prefix}{setup}{}\n{extra}", include_str!("local_http.php"));
    compare("curl-local-http", &script);
}
