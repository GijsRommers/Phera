//! Compare the same API script under PHP's development server and Phera.

mod web_support;

use std::path::{Path, PathBuf};

use phera_extension_api::testing::{oracle, scripts::Comparison, serve::Server};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/web")
}

fn application_response(raw: &str) -> (u16, Vec<(String, String)>, &str) {
    let (head, body) = raw.split_once("\r\n\r\n").expect("HTTP response");
    let mut lines = head.lines();
    let status = lines
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let mut headers: Vec<_> = lines
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            let name = name.to_ascii_lowercase();
            // These come from the HTTP transports, outside the PHP response head.
            if matches!(
                name.as_str(),
                "host"
                    | "date"
                    | "connection"
                    | "content-length"
                    | "x-powered-by"
                    | "transfer-encoding"
            ) {
                return None;
            }
            Some((name, value.trim().to_string()))
        })
        .collect();
    // The HTTP transports group different header names differently.
    // A stable sort keeps multiple Set-Cookie values in order.
    headers.sort_by(|left, right| left.0.cmp(&right.0));
    (status, headers, body)
}

#[test]
fn api_status_headers_and_body_match_php() {
    let php = oracle::find_php().expect("PHP 8.5 oracle");
    let root = fixtures();
    let script = root.join("api.php");
    let phera = Server::start(Path::new(env!("CARGO_BIN_EXE_phera")), &script, 1, &[]);
    let php = Server::start_php_router(&php, &root, &script);
    for (method, target, headers, body, status) in [
        (
            "GET",
            "/items?q=a+b&a%5Bx%5D=1",
            &[][..],
            b"".as_slice(),
            200,
        ),
        (
            "POST",
            "/items?q=save",
            &[("Content-Type", "application/x-www-form-urlencoded")],
            b"title=Le+Chat&tags[]=red&tags[]=blue",
            201,
        ),
        (
            "GET",
            "/items",
            &[("Cookie", "client=harry%20cat")],
            b"",
            200,
        ),
        ("GET", "/unknown", &[], b"", 404),
    ] {
        let expected = web_support::request(&php, "PHP", method, target, headers, body);
        let actual = web_support::request(&phera, "Phera", method, target, headers, body);
        let expected = application_response(&expected);
        let actual = application_response(&actual);
        assert_eq!(expected.0, status, "{method} {target}");
        assert_eq!(actual, expected, "{method} {target}");
    }
}

#[test]
fn request_buffers_stay_isolated_on_one_worker() {
    let directory = std::env::temp_dir().join(format!("phera-web-buffers-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let script = directory.join("index.php");
    std::fs::write(
        &script,
        r#"<?php
echo ob_get_level(), ':';
ob_start();
$id = $_GET['id'];
echo $id, ':';
if ($id === 'failed') {
    error_log('ready', 3, __DIR__ . '/ready');
    usleep(500000);
    throw new Exception('request failure');
}
echo ob_get_clean(), 'ok';
"#,
    )
    .unwrap();
    let server = Server::start(Path::new(env!("CARGO_BIN_EXE_phera")), &script, 1, &[]);
    let (failed, clean) = std::thread::scope(|scope| {
        let failed = scope.spawn(|| server.get_one("/?id=failed"));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !directory.join("ready").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "first request started"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let clean = server.get_one("/?id=clean");
        assert!(
            !failed.is_finished(),
            "second request completed while the first was suspended"
        );
        (failed.join().unwrap(), clean)
    });
    let (status, _, failed_body) = application_response(&failed);
    assert_eq!(status, 500);
    assert!(failed_body.starts_with("0:failed:"));
    assert!(failed_body.contains("request failure"));
    assert!(!failed_body.contains("clean"));
    assert_eq!(application_response(&clean).2, "0:clean:ok");
    let later = server.get_one("/?id=later");
    assert_eq!(application_response(&later).2, "0:later:ok");
    drop(server);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn cli_sapi_and_logging_match_php() {
    let compare = Comparison::for_binary("web-basics", Path::new(env!("CARGO_BIN_EXE_phera")));
    let script = compare.artifacts.join("cli.php");
    std::fs::write(
        &script,
        r#"<?php
var_dump(PHP_SAPI, php_sapi_name(), function_exists('getallheaders'));
var_dump(function_exists('apache_request_headers'));
var_dump(error_log('type 0'), error_log('type 4', 4));
var_dump(error_log("first\0", 3, $argv[1]), error_log("second\xff", 3, $argv[1]));
var_dump(error_log("file", 3, "file://" . $argv[1]));
chdir(dirname($argv[1]));
var_dump(error_log("relative", 3, basename($argv[1])));
"#,
    )
    .unwrap();
    let mut logs = Vec::new();
    compare
        .compare(&script, |runtime, command| {
            let log = compare
                .scratch_directory(&script, runtime)
                .join("error.log");
            command.arg(&log);
            logs.push(log);
        })
        .unwrap();
    for log in logs {
        assert_eq!(
            std::fs::read(log).unwrap(),
            b"first\0second\xfffilerelative"
        );
    }
}
