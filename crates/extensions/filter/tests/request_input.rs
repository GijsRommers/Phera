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
echo json_encode([
    filter_has_var(INPUT_GET, 'name'),
    filter_input(INPUT_GET, 'count', FILTER_VALIDATE_INT),
    filter_input(INPUT_GET, 'missing', FILTER_DEFAULT, FILTER_NULL_ON_FAILURE),
    filter_input_array(INPUT_GET, ['count' => FILTER_VALIDATE_INT, 'name' => FILTER_DEFAULT]),
    filter_input(INPUT_SERVER, 'REQUEST_METHOD'),
]);
"#,
    )
    .unwrap();
    let oracle = Server::start_php(&php, &root, 1);
    let candidate = Server::start(Path::new(&phera), &root, 1, &[]);
    for path in [
        "/index.php?count=3&name=first",
        "/index.php?count=8&name=second",
    ] {
        let php = oracle.get_one(path);
        let phera = candidate.get_one(path);
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
