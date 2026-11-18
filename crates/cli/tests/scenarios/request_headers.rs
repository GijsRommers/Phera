//! Request headers as a script sees them, compared with PHP's own server.
//!
//! The same requests go to `phera serve` and to `php -S`; a script prints
//! the request variables as JSON and both answers must be equal.

use phera_extension_api::testing::{oracle, serve::Server};

use super::super::{Served, raw_response};

/// Prints what the headers of a request became.
const SHOW: &str = r#"<?php
$server = [];
foreach ($_SERVER as $name => $value) {
    if (str_starts_with($name, 'HTTP_X') || str_contains($name, 'CONTENT_')
        || $name === 'HTTP_COOKIE' || $name === 'HTTP_ACCEPT') {
        $server[$name] = $value;
    }
}
echo json_encode(['server' => $server, 'post' => $_POST, 'cookie' => $_COOKIE]);
"#;

const FORM: &str = "Content-Type: application/x-www-form-urlencoded\r\n";

/// Method, extra header lines and body of each request.
const REQUESTS: &[(&str, &str, &str)] = &[
    ("GET", "X-Tag: one\r\nX-Tag: two\r\n", ""),
    ("GET", "X-Tag: one\r\nx-tag: two\r\nX-TAG: three\r\n", ""),
    ("GET", "X-Tag: one\r\nX-Tag:\r\nX-Tag: three\r\n", ""),
    // `X_Tag` is another header that lands on the same `$_SERVER` key.
    ("GET", "X_Tag: one\r\nX-Tag: two\r\n", ""),
    ("GET", "X-Tag: one\r\nX_Tag: two\r\nX-Tag: three\r\n", ""),
    ("GET", "X-B: 1\r\nX-A: 2\r\nX-B: 3\r\n", ""),
    (
        "GET",
        "Accept: text/html\r\nAccept: application/json\r\n",
        "",
    ),
    ("GET", "Cookie: a=1\r\nCookie: b=2; c=3\r\n", ""),
    ("GET", "Cookie: a=1, b=2\r\n", ""),
    ("GET", "Content-Type: text/plain\r\n", ""),
    ("POST", FORM, "a=1&b=2"),
    (
        "POST",
        "content-type: application/x-www-form-urlencoded\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: APPLICATION/X-WWW-FORM-URLENCODED\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded; charset=UTF-8\r\n",
        "a=1",
    ),
    // Two content types: the first one decides.
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded\r\nContent-Type: text/plain\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: text/plain\r\nContent-Type: application/x-www-form-urlencoded\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded; charset=UTF-8\r\n\
         Content-Type: text/plain\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded, text/plain\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded text/plain\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencoded\tx\r\n",
        "a=1",
    ),
    (
        "POST",
        "Content-Type: application/x-www-form-urlencodedx\r\n",
        "a=1",
    ),
    ("POST", "Content-Type: application/json\r\n", "{\"a\":1}"),
    ("POST", "", "a=1"),
    ("PUT", FORM, "a=1"),
    ("PATCH", FORM, "a=1"),
    ("DELETE", FORM, "a=1"),
];

#[test]
fn repeated_headers_and_form_bodies_match_php() {
    let php = oracle::find_php().expect("PHP 8.5 required");
    let ours = Served::start("request-headers", &[("show.php", SHOW)]);
    let theirs = Server::start_php(&php, &ours.root, 1);

    let mut differences = Vec::new();
    for (method, headers, body) in REQUESTS {
        let expected = answer_from_php(&theirs.address, method, headers, body);
        let actual = raw_response(&ours.address, method, "/show.php", headers, body)
            .map_or_else(|| "(no answer)".to_string(), |response| body_of(&response));
        if expected != actual {
            differences.push(format!(
                "{method} {headers:?} {body:?}\n  PHP:   {expected:?}\n  Phera: {actual:?}"
            ));
        }
    }
    assert!(differences.is_empty(), "{}", differences.join("\n"));
}

/// PHP's built-in server now and then closes a connection without
/// answering, mostly right after it started. The request is sent again; our
/// own server gets no such second chance.
fn answer_from_php(address: &str, method: &str, headers: &str, body: &str) -> String {
    for _ in 0..5 {
        if let Some(response) = raw_response(address, method, "/show.php", headers, body) {
            return body_of(&response);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    panic!("php -S gave no answer to {method} {headers:?}");
}

/// The part of a response after its headers.
fn body_of(response: &str) -> String {
    let (_, body) = response.split_once("\r\n\r\n").unwrap_or_default();
    body.to_string()
}

/// PHP's built-in server refuses a method it does not know, so there is
/// nothing to compare with. A method name is case-sensitive: `post` is not
/// `POST`, and its body is not a form submission.
#[test]
fn only_the_method_spelled_post_fills_post() {
    let script = "<?php echo $_SERVER['REQUEST_METHOD'], '|', $_POST['title'] ?? '-', '|', \
                  $_REQUEST['title'];";
    let served = Served::start("post-case", &[("form.php", script)]);
    for (method, expected) in [
        ("POST", "POST|body|body"),
        ("post", "post|-|query"),
        ("Post", "Post|-|query"),
    ] {
        assert_eq!(
            served.request(method, "/form.php?title=query", FORM, "title=body"),
            (200, expected.to_string())
        );
    }
}
