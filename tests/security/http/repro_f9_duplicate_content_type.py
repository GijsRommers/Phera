#!/usr/bin/env python3
"""F9: with two `Content-Type` request headers, Phera fills `$_POST` from the
FIRST header but reports the LAST one in `$_SERVER['CONTENT_TYPE']`.

`HttpRequest::header()` (`crates/runtime/src/http/mod.rs`) returns the first
match, which is what `is_form` checks; the `$_SERVER` builder
(`crates/runtime/src/http/superglobals.rs`) inserts every header in order, so
the last one wins there. The two disagree. Real PHP comma-joins duplicate
field lines (`Content-Type: a, b`) and matches the joined value, so with
`application/x-www-form-urlencoded` first and `application/json` second PHP
does NOT fill `$_POST` (verified against `/opt/homebrew/bin/php -n -S`).

An application that branches on `$_SERVER['CONTENT_TYPE']` (say, expecting
JSON and reading the raw body) can therefore see `$_POST` populated from a
header it was never told about. RFC 9110 section 5.5: duplicate singleton
fields should either be combined or rejected with 400.

Expect (matching PHP or rejecting): `$_POST` and `$_SERVER['CONTENT_TYPE']`
agree, or the request is a 400.
"""
import json
import sys

from lib import port_arg, report, send_raw, split_response, status_of

port = port_arg(sys.argv)
resp = send_raw(
    port,
    b"POST /post.php HTTP/1.1\r\nHost: x\r\nConnection: close\r\n"
    b"Content-Type: application/x-www-form-urlencoded\r\n"
    b"Content-Type: application/json\r\n"
    b"Content-Length: 3\r\n\r\na=1",
)
code, _ = status_of(resp)
_, _, body = split_response(resp)
try:
    data = json.loads(body.decode())
    ct = data["CT"]
    post = data["POST"]
except Exception:
    ct = post = None
vulnerable = code == 200 and post == {"a": "1"} and ct == "application/json"
sys.exit(
    report(
        "POST with Content-Type: form + Content-Type: application/json",
        vulnerable,
        f"status={code} $_SERVER['CONTENT_TYPE']={ct!r} $_POST={post!r} "
        f"(PHP: CONTENT_TYPE='application/x-www-form-urlencoded, application/json', $_POST=[])",
    )
)
