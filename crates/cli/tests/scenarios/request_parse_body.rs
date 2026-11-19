//! Compare `request_parse_body()` through both servers, with real HTTP methods,
//! bodies, the raw input stream, and the request's upload table.

use phera_extension_api::testing::{oracle, serve::Server};

use super::super::{Served, send_request};

const SHOW: &str = r#"<?php
header('Content-Type: text/plain');
$warnings = [];
set_error_handler(function ($level, $message) use (&$warnings) {
    $warnings[] = [$level, $message];
    return true;
});
if (isset($_GET['raw'])) {
    $raw = fopen('php://input', 'r');
    echo 'before:', strlen(fread($raw, (int) $_GET['raw'])), "\n";
}
$options = json_decode($_GET['options'] ?? 'null', true);
if (isset($_GET['globals'])) {
    $_POST['before'] = 'post';
    $_FILES['before'] = 'files';
}
for ($i = 0; $i < (isset($_GET['twice']) ? 2 : 1); $i++) {
    try {
        [$post, $files] = request_parse_body(options: $options);
        if (!empty($files['doc']['tmp_name'])) {
            $tmp = $files['doc']['tmp_name'];
            $files['doc']['tmp_name'] = [is_uploaded_file($tmp), file_get_contents($tmp)];
        }
        echo json_encode([$post, $files]), "\n";
    } catch (Throwable $e) {
        echo json_encode([get_class($e), $e->getMessage()]), "\n";
    }
    if (isset($tmp)) {
        echo 'upload:', (int) is_uploaded_file($tmp), ':', (int) file_exists($tmp), "\n";
    }
}
echo json_encode([$_POST, array_keys($_FILES), $warnings]), "\n";
echo 'after:', strlen(file_get_contents('php://input')), "\n";
if (isset($raw)) {
    echo 'old:', strlen(stream_get_contents($raw)), "\n";
}
if (isset($_GET['twice'], $tmp)) {
    unlink($tmp);
}
"#;

const URLENCODED: &str = "application/x-www-form-urlencoded";
const MULTIPART: &str = "multipart/form-data; boundary=boundary42";
const FORM: &str = "--boundary42\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nhello\r\n\
    --boundary42\r\nContent-Disposition: form-data; name=\"a[b][]\"\r\n\r\nworld\r\n\
    --boundary42\r\nContent-Disposition: form-data; name=\"doc\"; filename=\"note.txt\"\r\n\
    Content-Type: text/plain\r\n\r\nfile data\r\n--boundary42--\r\n";
const GARBLED: &str = "--boundary42\r\nContent-Disposition: form-data;\r\n\r\nfield\r\n\
    --boundary42--\r\n";
const FOLDED: &str = "--boundary42\r\nContent-Disposition: form-data;\r\n name=\"a\"\r\n\r\n\
    field\r\n--boundary42--\r\n";
const BARE_NAME: &str = "--boundary42\r\nContent-Disposition: form-data; name=a space\r\n\r\n\
    field\r\n--boundary42--\r\n";
const WRONG_HEADER: &str = "--boundary42\r\nContent-Disposition : form-data; name=\"ignored\"\r\n\
    \r\nfield\r\n--boundary42--\r\n";

#[test]
fn request_bodies_and_limits_match_the_php_server() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = Served::start("request-parse-body", &[("show.php", SHOW)]);
    let theirs = Server::start_php(&php, &ours.root, 1);
    let large = format!("a=1&b={}&c=3", "a".repeat(8192));
    let long_form = FORM.replace("world", &"a".repeat(6500));
    let long_garbled = GARBLED.replace("field", &"x".repeat(6500));
    let with_preamble = format!("not-a-delimiter--boundary42\r\n{WRONG_HEADER}{FOLDED}");
    let cases = [
        ("PUT", Some(URLENCODED), "a=1&b=2", ""),
        (
            "PATCH",
            Some(URLENCODED),
            "a[b][]=1&a[b][]=2&text=a+word",
            "",
        ),
        ("PUT", Some(URLENCODED), "a=1&&b=2&&", ""),
        ("PUT", Some(URLENCODED), "", ""),
        ("PUT", Some(URLENCODED), "a=1&a=2", "?twice=1"),
        ("PUT", Some(URLENCODED), "a=1&a=2", "?raw=1"),
        ("PUT", Some(URLENCODED), large.as_str(), "?raw=1"),
        ("POST", Some(URLENCODED), "a=1&b=2", ""),
        ("PUT", Some(MULTIPART), FORM, ""),
        ("PUT", Some(MULTIPART), FORM, "?raw=1"),
        ("PUT", Some(MULTIPART), FORM, "?twice=1"),
        ("PUT", Some(MULTIPART), FORM, "?globals=1"),
        ("POST", Some(MULTIPART), FORM, ""),
        ("PATCH", Some(MULTIPART), FORM, ""),
        ("DELETE", None, "", ""),
        ("DELETE", Some(""), "", ""),
        ("PUT", Some("application/json; charset=utf-8"), "{}", ""),
        ("PUT", Some("multipart/form-data"), FORM, ""),
        (
            "PUT",
            Some("multipart/form-data; boundary=\"boundary42"),
            FORM,
            "",
        ),
        (
            "PUT",
            Some("multipart/form-data; BOUNDARY=\"boundary42\""),
            FORM,
            "",
        ),
        ("PUT", Some(MULTIPART), GARBLED, ""),
        ("PUT", Some(MULTIPART), long_garbled.as_str(), ""),
        ("PUT", Some(MULTIPART), FOLDED, ""),
        ("PUT", Some(MULTIPART), BARE_NAME, ""),
        ("PUT", Some(MULTIPART), WRONG_HEADER, ""),
        ("PUT", Some(MULTIPART), with_preamble.as_str(), ""),
        (
            "PUT",
            Some(MULTIPART),
            long_form.as_str(),
            "?options=%7B%22max_multipart_body_parts%22%3A1%7D",
        ),
        (
            "PUT",
            Some(URLENCODED),
            "a=1&b=2",
            "?options=%7B%22post_max_size%22%3A1%7D&raw=1",
        ),
    ];
    for option in [
        "%7B%22post_max_size%22%3A1%7D",
        "%7B%22post_max_size%22%3A%221K%22%7D",
        "%7B%22max_input_vars%22%3A1%7D",
        "%7B%22max_input_vars%22%3A-1%7D",
        "%7B%22max_input_vars%22%3A0%7D",
        "%7B%22max_multipart_body_parts%22%3A2%7D",
        "%7B%22max_file_uploads%22%3A0%7D",
        "%7B%22upload_max_filesize%22%3A%222%22%7D",
        "%7B%22upload_max_filesize%22%3A0%7D",
        "%7B%22MAX_INPUT_VARS%22%3A1%2C%22max_input_vars%22%3A2%7D",
        "%7B%22bad_key%22%3A0%7D",
        "%7B%22max_input_vars%22%3Atrue%7D",
        "%7B%22max_input_vars%22%3A%221GB%22%7D",
    ] {
        let query = format!("?options={option}");
        let headers = |content_type: &str| format!("Content-Type: {content_type}\r\n");
        for (content_type, body) in [(URLENCODED, "a=1&b=2&c=3"), (MULTIPART, FORM)] {
            let headers = headers(content_type);
            compare(&ours, &theirs, "PUT", &headers, body, &query);
        }
    }
    for (method, content_type, body, query) in cases {
        let headers =
            content_type.map_or(String::new(), |value| format!("Content-Type: {value}\r\n"));
        compare(&ours, &theirs, method, &headers, body, query);
    }
}

fn compare(ours: &Served, theirs: &Server, method: &str, headers: &str, body: &str, query: &str) {
    let target = format!("/show.php{query}");
    let expected = send_request(&theirs.address, method, &target, headers, body);
    let actual = ours.request(method, &target, headers, body);
    assert_eq!(
        actual,
        expected,
        "{method} {target}, {headers}, {} bytes",
        body.len()
    );
}

#[test]
fn request_parse_body_uploads_can_be_moved_and_unclaimed_files_are_deleted() {
    const MOVE: &str = r#"<?php
    [, $files] = request_parse_body();
    $tmp = $files['doc']['tmp_name'];
    $to = __DIR__ . '/moved';
    var_dump(is_uploaded_file($tmp), move_uploaded_file($tmp, $to), is_uploaded_file($tmp),
        file_get_contents($to));
    unlink($to);
    "#;
    const LEAVE: &str = "<?php [, $files] = request_parse_body(); echo $files['doc']['tmp_name'];";
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = Served::start(
        "request-upload",
        &[("move.php", MOVE), ("leave.php", LEAVE)],
    );
    let theirs = Server::start_php(&php, &ours.root, 1);
    let headers = format!("Content-Type: {MULTIPART}\r\n");
    assert_eq!(
        ours.request("PUT", "/move.php", &headers, FORM),
        send_request(&theirs.address, "PUT", "/move.php", &headers, FORM),
    );
    let (status, path) = ours.request("PUT", "/leave.php", &headers, FORM);
    assert_eq!(status, 200, "{path}");
    let path = std::path::Path::new(&path);
    assert!(path.is_absolute());
    assert!(
        !path.exists(),
        "upload survived the request: {}",
        path.display()
    );
}
