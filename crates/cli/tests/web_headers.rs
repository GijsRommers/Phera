//! Cookie return values, diagnostics and response lines under the two web SAPIs.

use std::path::Path;

use phera_extension_api::testing::{oracle, serve::Server};

const COOKIES: &str = r#"<?php
header_remove('X-Powered-By');
$warnings = [];
set_error_handler(function ($level, $message) use (&$warnings) {
    $warnings[] = [$level, $message];
    return true;
});
$results = [];
$batch = (int) ($_GET['batch'] ?? 0);
$names = ['n', '', 'n=r', 'n n', "n\0x", '[]'];
$values = ['', '1', '1 =2', ';', "\xff\0", "one\t", '~@+='];
$options = [0, 1, PHP_INT_MAX, [], ['PATH' => '/', 'domain' => 'example.test'],
    ['secure' => true, 'partitioned' => true, 'samesite' => 'None'], ['partitioned' => true],
    ['expires' => 1, 'httponly' => '0'], ['path' => "bad\npath"], ['domain' => 'bad, domain'],
    ['samesite' => "bad\nsite"], ['notAnOption' => true], [1 => 1], 'not numeric'];
foreach (['setcookie', 'setrawcookie'] as $function) {
    foreach ($names as $index => $name) {
        if ($index !== $batch) {
            continue;
        }
        foreach ($values as $value) {
            foreach ($options as $option) {
                header_remove();
                $warnings = [];
                try {
                    $return = $function($name, $value, $option);
                } catch (Throwable $exception) {
                    $return = [get_class($exception), $exception->getMessage()];
                }
                $results[] = json_encode([$return, $warnings, array_map('bin2hex', headers_list())]);
            }
        }
    }
    if ($batch !== count($names)) {
        continue;
    }
    foreach ([
        [],
        ['sid'],
        ['sid', '', 0, '/', 'example.test', true, true],
        ['sid', 'abc', [], '/'],
        [[], 'abc'],
        ['sid', []],
        ['sid', 'abc', 1, null],
        ['sid', 'abc', false, '/', '', [], true],
    ] as $args) {
        header_remove();
        $warnings = [];
        try {
            $return = $function(...$args);
        } catch (Throwable $exception) {
            $return = [get_class($exception), $exception->getMessage()];
        }
        $results[] = json_encode([$return, $warnings, array_map('bin2hex', headers_list())]);
    }
}
header_remove();
header('Content-Type: application/json');
echo implode("\n", $results);
"#;

const REQUEST: &str = r#"<?php
header_remove('X-Powered-By');
header_register_callback(function () {
    $GLOBALS['cb'] = [headers_sent(), headers_list()];
    header('X-Callback: ready');
});
function lowercase_keys(array $headers): array {
    $result = [];
    foreach ($headers as $name => $value) {
        $result[strtolower($name)] = $value;
    }
    return $result;
}
$names = lowercase_keys(getallheaders());
$alias = lowercase_keys(apache_request_headers());
flush();
$result = [
    PHP_SAPI, php_sapi_name(),
    function_exists('getallheaders'), function_exists('apache_request_headers'),
    $names['x-client'], $names === $alias,
    $GLOBALS['cb'],
    headers_sent($file, $line),
    $file, $line,
];
set_error_handler(function ($level, $message) use (&$result) {
    $result[] = [$level, $message];
    return true;
});
$result[] = setcookie('too-late', 'value');
echo json_encode($result);
"#;

fn compare(body: &str, name: &str, targets: &[&str], headers: &[(&str, &str)]) -> (String, String) {
    let php = oracle::find_php().expect("PHP 8.5 oracle");
    let root = std::env::temp_dir().join(format!("phera-web-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("test.php");
    std::fs::write(&script, body).unwrap();
    let php = Server::start_php_router(&php, &root, &script);
    let phera = Server::start(Path::new(env!("CARGO_BIN_EXE_phera")), &script, 1, &[]);
    let [expected, actual] = [("PHP", &php), ("Phera", &phera)].map(|(runtime, server)| {
        let mut output = String::new();
        for target in targets {
            let mut response = server.request("GET", target, headers, &[]);
            // The two scripts are read-only requests. Retry only a close
            // before any response bytes arrive.
            for _ in 0..2 {
                if !response.is_empty() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                response = server.request("GET", target, headers, &[]);
            }
            output.push_str(response_body(runtime, target, &response));
            output.push('\n');
        }
        output
    });
    drop(php);
    drop(phera);
    std::fs::remove_dir_all(root).unwrap();
    (expected, actual)
}

fn response_body<'a>(runtime: &str, target: &str, response: &'a str) -> &'a str {
    let (headers, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("{runtime} {target} returned incomplete HTTP: {response:?}"));
    assert!(
        headers.starts_with("HTTP/1.1 200"),
        "{runtime} {target}: {response}"
    );
    body
}

#[test]
fn cookie_options_and_validation_match_php() {
    // Keep each response small while still comparing every call in the matrix.
    let targets = [
        "/?batch=0",
        "/?batch=1",
        "/?batch=2",
        "/?batch=3",
        "/?batch=4",
        "/?batch=5",
        "/?batch=6",
    ];
    let (expected, actual) = compare(COOKIES, "cookies", &targets, &[]);
    let expected: Vec<_> = expected.lines().collect();
    let actual: Vec<_> = actual.lines().collect();
    assert_eq!(expected.len(), 1192);
    assert_eq!(actual.len(), expected.len());
    for (index, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
        assert_eq!(actual, expected, "cookie oracle case {index}");
    }
}

#[test]
fn web_sapi_header_alias_and_flush_match_php() {
    let (expected, actual) = compare(
        REQUEST,
        "sapi",
        &["/"],
        &[("X-Client", "first"), ("X-Client", "last")],
    );
    // Phera deliberately names the web SAPI "phera".
    let expected = expected.replacen(r#"["cli-server","cli-server","#, r#"["phera","phera","#, 1);
    assert_eq!(actual, expected);
}
