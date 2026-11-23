//! The input filters see raw request values even after PHP changes its globals.

use std::{fs, path::Path};

use phera_extension_api::testing::{oracle::find_php, serve::Server};

#[test]
fn request_filters_match_php_across_requests() {
    let (Some(phera), Some(php)) = (std::env::var_os("PHERA_BIN"), find_php()) else {
        return;
    };
    let root = std::env::temp_dir().join(format!("phera-filter-input-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("index.php"),
        r#"<?php
if (function_exists('Phera\snapshot')) {
    Phera\snapshot();
}
$_GET['count'] = 99;
unset($_GET['name']);
$serverKeys = array_keys(filter_input_array(INPUT_SERVER) ?? []);
sort($serverKeys);
$syntheticKeys = [];
foreach (['REQUEST_URI', 'REQUEST_METHOD', 'PATH_INFO', 'PHP_SELF', 'SERVER_NAME', 'REMOTE_ADDR'] as $key) {
    $syntheticKeys[$key] = [
        filter_has_var(INPUT_SERVER, $key),
        filter_input(INPUT_SERVER, $key),
    ];
}
echo json_encode([
    filter_has_var(INPUT_GET, 'name'),
    filter_input(INPUT_GET, 'count', FILTER_VALIDATE_INT),
    filter_input(INPUT_GET, 'missing', FILTER_DEFAULT, FILTER_NULL_ON_FAILURE),
    filter_input_array(INPUT_GET, ['count' => FILTER_VALIDATE_INT, 'name' => FILTER_DEFAULT]),
    $serverKeys,
    $syntheticKeys,
    filter_input(INPUT_SERVER, 'HTTP_HOST'),
    filter_input(INPUT_SERVER, 'QUERY_STRING'),
    $_REQUEST['count'],
]);
"#,
    )
    .unwrap();
    let oracle = Server::start_php(&php, &root, 1);
    let candidate = Server::start(Path::new(&phera), &root, 1, &[]);
    for (method, path, headers, body) in [
        (
            "GET",
            "/index.php?count=3&name=first",
            &[("X-Filter-Test", "request")][..],
            &b""[..],
        ),
        (
            "POST",
            "/index.php?count=8&name=second",
            &[
                ("X-Filter-Test", "request"),
                ("Content-Type", "application/x-www-form-urlencoded"),
            ][..],
            &b"key=value"[..],
        ),
    ] {
        let php = oracle.request(method, path, headers, body);
        let phera = candidate.request(method, path, headers, body);
        assert!(php.starts_with("HTTP/1.1 200"), "{php}");
        assert!(phera.starts_with("HTTP/1.1 200"), "{phera}");
        assert_eq!(
            php.split_once("\r\n\r\n").unwrap().1,
            phera.split_once("\r\n\r\n").unwrap().1
        );
    }
    drop(candidate);
    drop(oracle);
    fs::remove_dir_all(root).unwrap();
}
