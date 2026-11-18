//! File uploads through `phera serve`, compared with PHP's own server: the
//! same `multipart/form-data` requests go to both, and a script prints
//! `$_POST`, `$_FILES` and what `move_uploaded_file()` does.

use phera_extension_api::testing::{oracle, serve::Server};

use super::super::{Served, raw_response};

/// Prints the request's form data. A temporary name differs between the
/// servers, so it is replaced by a checksum of the file it names.
const SHOW: &str = r#"<?php
header('Content-Type: text/plain');
function describe(array $files): array {
    array_walk_recursive($files, function (&$value) {
        if (is_string($value) && $value !== '' && is_uploaded_file($value)) {
            $value = 'uploaded:' . md5(file_get_contents($value));
        }
    });
    return $files;
}
echo json_encode(['post' => $_POST, 'files' => describe($_FILES),
    'raw' => strlen(file_get_contents('php://input'))]), "\n";
$tmp = $_FILES['doc']['tmp_name'] ?? '';
if (is_string($tmp) && $tmp !== '') {
    $to = sys_get_temp_dir() . '/phera-upload-test-' . $_GET['server'] . '.moved';
    var_dump(is_uploaded_file($tmp), is_uploaded_file(__FILE__), move_uploaded_file(__FILE__, $to),
        move_uploaded_file($tmp, $to), file_exists($tmp), is_uploaded_file($tmp),
        file_get_contents($to), move_uploaded_file($tmp, $to), decoct(fileperms($to) & 0777));
    unlink($to);
}
"#;

const BOUNDARY: &str = "----bnd42";

/// One part: the value of `Content-Disposition`, an optional content type,
/// and the content.
type Part<'a> = (&'a str, Option<&'a str>, &'a str);

fn form(parts: &[Part<'_>], closed: bool) -> String {
    let mut body = String::new();
    for (disposition, content_type, content) in parts {
        body.push_str(&format!(
            "--{BOUNDARY}\r\nContent-Disposition: {disposition}\r\n"
        ));
        if let Some(content_type) = content_type {
            body.push_str(&format!("Content-Type: {content_type}\r\n"));
        }
        body.push_str(&format!("\r\n{content}\r\n"));
    }
    if closed {
        body.push_str(&format!("--{BOUNDARY}--\r\n"));
    }
    body
}

fn cases() -> Vec<(&'static str, String)> {
    let too_large = "x".repeat(2 * 1024 * 1024 + 10);
    let at_the_limit = "y".repeat(2 * 1024 * 1024);
    let many: Vec<(String, String)> = (0..22)
        .map(|index| {
            (
                format!("form-data; name=\"m{index}\"; filename=\"{index}.txt\""),
                index.to_string(),
            )
        })
        .collect();
    let many: Vec<Part<'_>> = many
        .iter()
        .map(|(disposition, content)| (disposition.as_str(), None, content.as_str()))
        .collect();
    let looks_like_boundary = format!("line1\r\n--{BOUNDARY} not a boundary line\r\nmore");
    let boundary_inside = format!("line1 --{BOUNDARY} inside");
    vec![
        (
            "a field and a file",
            form(
                &[
                    ("form-data; name=\"title\"", None, "hello"),
                    (
                        "form-data; name=\"doc\"; filename=\"notes.txt\"",
                        Some("text/plain"),
                        "file body\n",
                    ),
                ],
                true,
            ),
        ),
        (
            "no file chosen",
            form(
                &[("form-data; name=\"doc\"; filename=\"\"", None, "")],
                true,
            ),
        ),
        (
            "several files under one name",
            form(
                &[
                    (
                        "form-data; name=\"docs[]\"; filename=\"a.txt\"",
                        Some("text/plain"),
                        "A",
                    ),
                    (
                        "form-data; name=\"docs[]\"; filename=\"b.bin\"",
                        None,
                        "\0\u{1}\u{2}",
                    ),
                    ("form-data; name=\"docs[]\"; filename=\"\"", None, ""),
                ],
                true,
            ),
        ),
        (
            "nested names",
            form(
                &[
                    (
                        "form-data; name=\"up[main][x]\"; filename=\"deep.txt\"",
                        Some("text/x; charset=utf-8"),
                        "deep",
                    ),
                    (
                        "form-data; name=\"up[other]\"; filename=\"o.txt\"",
                        None,
                        "o",
                    ),
                    ("form-data; name=\"f[a][b]\"", None, "field"),
                ],
                true,
            ),
        ),
        (
            "paths, quotes and bare values in file names",
            form(
                &[
                    (
                        "form-data; name=\"doc\"; filename=\"C:\\\\dir\\\\win.txt\"",
                        None,
                        "w",
                    ),
                    (
                        "form-data; name=\"p1\"; filename=\"/etc/pass wd\"",
                        None,
                        "u",
                    ),
                    ("form-data; name=\"p2\"; filename=\"a\\\"b.txt\"", None, "q"),
                    ("form-data; name=p3; filename=bare.txt", None, "b"),
                    ("form-data; name=\"p4\"; filename='single.txt'", None, "s"),
                ],
                true,
            ),
        ),
        (
            "larger than the server allows",
            form(
                &[
                    (
                        "form-data; name=\"big\"; filename=\"big.bin\"",
                        None,
                        &too_large,
                    ),
                    ("form-data; name=\"after\"", None, "still here"),
                ],
                true,
            ),
        ),
        (
            "exactly as large as allowed",
            form(
                &[(
                    "form-data; name=\"edge\"; filename=\"edge.bin\"",
                    None,
                    &at_the_limit,
                )],
                true,
            ),
        ),
        (
            "the form's own limit",
            form(
                &[
                    ("form-data; name=\"MAX_FILE_SIZE\"", None, "5"),
                    (
                        "form-data; name=\"small\"; filename=\"s.txt\"",
                        None,
                        "12345",
                    ),
                    (
                        "form-data; name=\"over\"; filename=\"o.txt\"",
                        None,
                        "123456",
                    ),
                ],
                true,
            ),
        ),
        (
            "a name twice, and names PHP rewrites",
            form(
                &[
                    ("form-data; name=\"doc\"; filename=\"1.txt\"", None, "one"),
                    ("form-data; name=\"doc\"; filename=\"2.txt\"", None, "two"),
                    ("form-data; name=\"t\"", None, "1"),
                    ("form-data; name=\"t\"", None, "2"),
                    ("form-data; name=\"a.b c\"", None, "v"),
                    ("form-data; name=\"up.load\"; filename=\"d.txt\"", None, "d"),
                    ("form-data; name=\"l[]\"", None, "1"),
                    ("form-data; name=\"l[]\"", None, "2"),
                ],
                true,
            ),
        ),
        (
            "cut off in the middle of a file",
            form(
                &[
                    ("form-data; name=\"title\"", None, "cut"),
                    (
                        "form-data; name=\"doc\"; filename=\"cut.txt\"",
                        None,
                        "partial",
                    ),
                ],
                false,
            ),
        ),
        (
            "files without a control name",
            form(
                &[
                    ("form-data; filename=\"anon.txt\"", None, "x"),
                    ("form-data; name=\"ok\"", None, "1"),
                    ("form-data; filename=\"anon2.txt\"", None, "yy"),
                ],
                true,
            ),
        ),
        ("an empty body", String::new()),
        (
            "line breaks and a boundary-like line inside a file",
            form(
                &[(
                    "form-data; name=\"doc\"; filename=\"lines.dat\"",
                    None,
                    "line1\r\nline2\r\n--not-the-boundary\r\n\0",
                )],
                true,
            ),
        ),
        ("more files than allowed", form(&many, true)),
        (
            "the boundary at the start of a line, with more after it",
            form(
                &[("form-data; name=\"a\"", None, &looks_like_boundary)],
                true,
            ),
        ),
        (
            "the boundary in the middle of a line",
            form(&[("form-data; name=\"a\"", None, &boundary_inside)], true),
        ),
        (
            "spaces around the equals sign",
            form(
                &[(
                    "form-data ;  name = \"a\" ; filename = \"s p.txt\"",
                    None,
                    "1",
                )],
                true,
            ),
        ),
        (
            "empty value, empty name, empty file",
            form(
                &[
                    ("form-data; name=\"e\"", None, ""),
                    ("form-data; name=\"\"", None, "v"),
                    ("form-data; name=\"f\"; filename=\"e.txt\"", None, ""),
                ],
                true,
            ),
        ),
    ]
}

/// PHP prints its start-up warnings (too many files, no boundary) into the
/// page as HTML; Phera does not put them in the response.
fn without_startup_warnings(body: &str) -> String {
    body.lines()
        .filter(|line| !line.starts_with("<br />") && !line.starts_with("<b>Warning</b>"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn answer(address: &str, server: &str, method: &str, content_type: &str, body: &str) -> String {
    let headers = format!("Content-Type: {content_type}\r\n");
    let target = format!("/show.php?server={server}");
    // PHP's server now and then closes a connection unanswered; ask again.
    for _ in 0..5 {
        if let Some(response) = raw_response(address, method, &target, &headers, body) {
            let (_, body) = response.split_once("\r\n\r\n").unwrap_or_default();
            return without_startup_warnings(body);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    "(no answer)".to_string()
}

#[test]
fn uploads_fill_post_and_files_as_in_php() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = Served::start("uploads", &[("show.php", SHOW)]);
    let theirs = Server::start_php(&php, &ours.root, 1);
    let multipart = format!("multipart/form-data; boundary={BOUNDARY}");

    let mut requests: Vec<(String, &str, String, String)> = cases()
        .into_iter()
        .map(|(name, body)| (name.to_string(), "POST", multipart.clone(), body))
        .collect();
    let small = form(
        &[
            ("form-data; name=\"title\"", None, "t"),
            ("form-data; name=\"doc\"; filename=\"n.txt\"", None, "n"),
        ],
        true,
    );
    for (name, method, content_type) in [
        (
            "a quoted boundary",
            "POST",
            format!("multipart/form-data; boundary=\"{BOUNDARY}\""),
        ),
        (
            "upper case",
            "POST",
            format!("MULTIPART/FORM-DATA; BOUNDARY={BOUNDARY}"),
        ),
        (
            "a parameter before the boundary",
            "POST",
            format!("multipart/form-data; charset=utf-8; boundary={BOUNDARY}"),
        ),
        ("no boundary", "POST", "multipart/form-data".to_string()),
        (
            "another boundary",
            "POST",
            "multipart/form-data; boundary=other".to_string(),
        ),
        ("PUT is not read as a form", "PUT", multipart.clone()),
    ] {
        requests.push((name.to_string(), method, content_type, small.clone()));
    }

    let mut differences = Vec::new();
    for (name, method, content_type, body) in &requests {
        let expected = answer(&theirs.address, "php", method, content_type, body);
        let actual = answer(&ours.address, "phera", method, content_type, body);
        if expected != actual {
            differences.push(format!("{name}\n  PHP:   {expected}\n  Phera: {actual}"));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// What a script does not move is gone when the request ends.
#[test]
fn unclaimed_uploads_are_deleted_after_the_request() {
    let script = "<?php echo $_FILES['doc']['tmp_name'], '|', \
                  (int) file_exists($_FILES['doc']['tmp_name']);";
    let served = Served::start("uploads-cleanup", &[("keep.php", script)]);
    let body = form(
        &[("form-data; name=\"doc\"; filename=\"n.txt\"", None, "n")],
        true,
    );
    let headers = format!("Content-Type: multipart/form-data; boundary={BOUNDARY}\r\n");
    let (status, answer) = served.request("POST", "/keep.php", &headers, &body);
    let (path, existed) = answer.split_once('|').expect("a path and a flag");
    assert_eq!((status, existed), (200, "1"));
    assert!(
        !std::path::Path::new(path).exists(),
        "{path} is still there"
    );
}
